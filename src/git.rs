//! Installed Git is the semantics authority. Object browsing never checks out a revision.

mod file_history;
mod history_detail_v994;
pub use history_detail_v994::{
    HistoryChangedFileV994, HistoryChangedFilesV994, HISTORY_DETAIL_FILES_V994,
};
mod history_query_v1;
pub use history_query_v1::{
    history_next_match_v1, history_prev_match_v1, HistoryCommitV1, HistoryFilterModeV1,
    HistoryFilterV1, HistoryOrderV1, HistoryPageV1, HistoryQueryV1, HistoryRefKindV1,
    HistoryRefSnapshotV1, HistoryRefV1, HistoryScopeV1, HistorySessionV1,
};
mod local_management_v41;
pub use local_management_v41::{
    bisect_only_skipped_v44, bisect_progress_v44, BisectProgressV44, StashContentsV41,
};
mod stash_recovery_v1;
pub use stash_recovery_v1::{FailedStashIntentV1, StashAttemptFailureV1, StashAttemptSuccessV2};
pub(crate) mod original_comparison_sides_v1;
pub use original_comparison_sides_v1::{
    OriginalChangedFileV1, OriginalComparisonEndpointV1, OriginalComparisonIntentV1,
    OriginalComparisonSideV1, OriginalComparisonSourceV1,
};
pub(crate) mod original_tree_inventory_v1;
pub use original_tree_inventory_v1::{
    OriginalTreeIntentV1, OriginalTreeInventoryV1, OriginalTreeKindV1, OriginalTreeMemberV1,
};
mod remote_handoff_read_v1;
pub(crate) mod workflow_edit_v9;
use remote_handoff_read_v1::{run_git_with_window_v1, GitReadPolicyV1};
mod local_commit_actor_origin_v1;
mod local_saved_navigation_origin_v1;
mod native_full_side_v3;
mod native_mutable_diff_source_v2;
mod readonly_diff_paths_v1;
pub(crate) use local_commit_actor_origin_v1::original_saved_sha as original_actor_saved_sha;
pub use local_commit_actor_origin_v1::OriginalLocalActorOriginV1;
#[cfg(test)]
pub(crate) use local_commit_actor_origin_v1::{
    register_fixture_trace as register_actor_origin_test_trace,
    FixtureTraceGuard as ActorOriginTraceGuard,
};
pub(crate) use native_full_side_v3::MutableFullSideV3;
pub use native_mutable_diff_source_v2::{
    MutableChangedFileV2, MutableDiffIntentV2, MutableDiffKindV2, MutableDiffSourceV2,
};
mod local_commit_actor_v1;
pub use file_history::{FileBlameLine, FileHistoryEntry, FileHistoryPage};
#[path = "git/original_graph_page_v1.rs"]
mod original_graph_page_v1;
pub use local_commit_actor_v1::{
    LocalCommitActorLimits, LocalCommitSourceIdentity, OriginalGitActorHeader,
    OriginalLocalCommitActors,
};
pub use original_graph_page_v1::{
    FollowedNameV993, FollowedPathsV993, OriginalGraphCommitV1, OriginalGraphEpochV1,
    OriginalGraphPageV1, OriginalGraphReadV1,
};

use crate::diff::Diff;
use crate::providers::runner::{CancellationToken, CommandRunner, CommandSpec, FailureKind};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const MAX_OUTPUT: usize = 32 * 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
static RUNNER: OnceLock<CommandRunner> = OnceLock::new();
static MUTATION_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
type NativeSelectedExecutionV2<'a> = (
    &'a crate::repository_native_mutable_plan_v2::NativeMutableConfirmRequestV2,
    &'a crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitError(pub String);

impl fmt::Display for GitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for GitError {}

impl From<std::io::Error> for GitError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

pub type GitResult<T> = Result<T, GitError>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RawPath(pub Vec<u8>);

impl RawPath {
    pub fn display(&self) -> String {
        display_path_bytes(&self.0)
    }

    pub fn as_os_str(&self) -> &OsStr {
        OsStr::from_bytes(&self.0)
    }

    pub fn new(value: impl Into<Vec<u8>>) -> Self {
        Self(value.into())
    }
}

/// Unambiguous path labels: invalid byte escapes and literal backslashes cannot collide with
/// genuine Unicode replacement characters or escaped newline/tab/control representations.
pub fn display_path_bytes(bytes: &[u8]) -> String {
    fn append_valid(value: &str, output: &mut String) {
        for character in value.chars() {
            match character {
                '\\' => output.push_str("\\\\"),
                '\n' => output.push_str("\\n"),
                '\r' => output.push_str("\\r"),
                '\t' => output.push_str("\\t"),
                character
                    if character.is_control()
                        || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
                {
                    output.push_str(&format!("\\u{{{:x}}}", character as u32))
                }
                character => output.push(character),
            }
        }
    }
    let mut remaining = bytes;
    let mut output = String::new();
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(valid) => {
                append_valid(valid, &mut output);
                break;
            }
            Err(error) => {
                let prefix = error.valid_up_to();
                append_valid(
                    std::str::from_utf8(&remaining[..prefix])
                        .expect("UTF-8 error identifies valid prefix"),
                    &mut output,
                );
                remaining = &remaining[prefix..];
                let invalid = error.error_len().unwrap_or(remaining.len());
                for byte in &remaining[..invalid] {
                    output.push_str(&format!("\\x{byte:02x}"));
                }
                remaining = &remaining[invalid..];
            }
        }
    }
    output
}

/// Removes controls from labels; the underlying identifiers remain untouched.
pub fn display_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .flat_map(|ch| match ch {
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            ch if ch.is_control()
                || matches!(ch,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}') =>
            {
                format!("\\u{{{:x}}}", ch as u32).chars().collect()
            }
            ch => vec![ch],
        })
        .collect()
}

/// A display-only transport target. Raw remote URLs remain intact for Git/state binding;
/// userinfo, auth-bearing query/fragment suffixes and known tokens never reach previews.
pub fn display_remote_url(value: &str) -> String {
    let transport_url = value.contains("://")
        || value
            .rsplit_once('@')
            .is_some_and(|(_, target)| target.contains(':'));
    let mut safe = if let Some((scheme, rest)) = value.split_once("://") {
        let (authority, path) = rest
            .find('/')
            .map_or((rest, ""), |split| (&rest[..split], &rest[split..]));
        let authority = authority.rsplit_once('@').map_or_else(
            || authority.to_owned(),
            |(_, host)| format!("[userinfo redacted]@{host}"),
        );
        format!("{scheme}://{authority}{path}")
    } else if !value.starts_with('/') && !value.starts_with('.') {
        match value
            .rsplit_once('@')
            .and_then(|(_, target)| target.split_once(':'))
        {
            Some((host, path)) => format!("[userinfo redacted]@{host}:{path}"),
            _ => value.to_owned(),
        }
    } else {
        value.to_owned()
    };
    if transport_url {
        if let Some(split) = safe.find(['?', '#']) {
            safe.truncate(split);
            safe.push_str("[query/fragment redacted]");
        }
    }
    display_bytes(crate::providers::runner::redact(&safe, &[]).as_bytes())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub path: RawPath,
    pub original_path: Option<RawPath>,
    pub index: char,
    pub worktree: char,
    pub conflict: bool,
    pub submodule: bool,
}

impl StatusEntry {
    pub fn staged(&self) -> bool {
        !matches!(self.index, '.' | '?' | '!') && !self.conflict
    }

    pub fn untracked(&self) -> bool {
        self.index == '?'
    }

    pub fn label(&self) -> &'static str {
        if self.conflict {
            "Conflict"
        } else if self.untracked() {
            "Untracked"
        } else if self.index == 'R' || self.worktree == 'R' {
            "Renamed"
        } else if self.index == 'D' || self.worktree == 'D' {
            "Deleted"
        } else if self.index == 'A' {
            "Added"
        } else {
            "Modified"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepoStatus {
    pub head: Option<String>,
    pub branch: Option<String>,
    pub raw_branch: Option<Vec<u8>>,
    pub detached: bool,
    pub upstream: Option<String>,
    pub raw_upstream: Option<Vec<u8>>,
    pub ahead: usize,
    pub behind: usize,
    pub entries: Vec<StatusEntry>,
    pub operation: Option<Sequencer>,
    pub shallow: bool,
    pub sparse: bool,
    pub replacement_refs: usize,
    pub grafted: bool,
}

impl RepoStatus {
    pub fn has_history_overrides(&self) -> bool {
        self.replacement_refs > 0 || self.grafted
    }
    pub fn clean(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn conflicts(&self) -> usize {
        self.entries.iter().filter(|entry| entry.conflict).count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitActor {
    pub name: String,
    pub email: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub id: String,
    pub parents: Vec<String>,
    pub author: GitActor,
    pub committer: GitActor,
    pub subject: String,
    pub refs: Vec<String>,
    pub raw_refs: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub name: String,
    pub raw_name: Vec<u8>,
    pub id: String,
    pub upstream: Option<String>,
    pub raw_upstream: Option<Vec<u8>>,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRelationship {
    pub base: String,
    pub head: String,
    /// Saved commits reachable only from head, not base.
    pub ahead: usize,
    /// Saved commits reachable only from base, not head.
    pub behind: usize,
    /// All best common starting points. Empty means none is available in loaded history.
    pub common_ancestors: Vec<String>,
    pub shallow: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: RawPath,
    pub mode: String,
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparison {
    /// Working files compared to the index: unstaged edits.
    Working,
    /// Working files compared to HEAD: all uncommitted tracked edits.
    Uncommitted,
    /// The index compared to HEAD: contents of the next commit.
    Staged,
    /// Complete saved snapshots at A and B, equivalent to `git diff A B`.
    Snapshots { base: String, head: String },
    /// Common starting point of A/B compared to B. Directional.
    BranchChanges { base: String, head: String },
    /// Changes introduced by one original saved commit against its explicitly selected parent.
    /// A root commit compares to the empty tree; parent zero is the first parent.
    CommitChanges { commit: String, parent: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonResult {
    pub comparison: Comparison,
    pub description: String,
    pub base: Option<String>,
    pub head: Option<String>,
    pub common_ancestor: Option<String>,
    pub diff: Diff,
    pub patch: Vec<u8>,
    /// Exact local state represented by a working/staged diff, for stale-selection rejection.
    pub observed: Option<WorkspaceFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    pub commit: String,
    pub original_line: u32,
    pub final_line: u32,
    pub author: String,
    pub email: String,
    pub path: RawPath,
    pub text: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sequencer {
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Bisect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReflogEntry {
    pub id: String,
    pub selector: String,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stash {
    pub id: String,
    pub selector: String,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub locked: bool,
    pub prunable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictStage {
    pub stage: u8,
    pub mode: String,
    pub id: String,
    pub path: RawPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullStrategy {
    FastForwardOnly,
    Merge,
    Rebase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Stage(Vec<RawPath>),
    Unstage(Vec<RawPath>),
    ApplyPatch {
        patch: Vec<u8>,
        reverse: bool,
    },
    Commit {
        message: String,
        amend: bool,
    },
    CreateBranch {
        name: String,
        revision: String,
    },
    RenameBranch {
        old: String,
        new: String,
    },
    SwitchBranch(String),
    DeleteBranch {
        name: String,
        force: bool,
    },
    CreateTag {
        name: String,
        revision: String,
    },
    DeleteTag(String),
    SetUpstream {
        branch: String,
        upstream: String,
    },
    Fetch {
        remote: String,
    },
    Pull {
        remote: String,
        branch: String,
    },
    PullWithStrategy {
        remote: String,
        branch: String,
        strategy: PullStrategy,
    },
    Push {
        remote: String,
        branch: String,
        set_upstream: bool,
    },
    ForcePushLease {
        remote: String,
        branch: String,
        expected_remote: String,
    },
    StashSave {
        message: String,
        untracked: bool,
    },
    StashApply {
        id: String,
        pop: bool,
    },
    StashDrop {
        id: String,
    },
    AddWorktree {
        path: PathBuf,
        branch: String,
    },
    /// Explicit new branch at an exact already-reviewed saved commit. W9's
    /// closed controller adds current source/target/lock checks; this is not
    /// a remote source, force replacement or saved-path capability.
    AddWorktreeBranchAtV9 {
        path: PathBuf,
        branch: String,
        commit: String,
    },
    RemoveWorktree(PathBuf),
    Merge(String),
    Rebase(String),
    CherryPick(String),
    Revert(String),
    Continue,
    Skip,
    Abort,
    RecoverBranch {
        name: String,
        revision: String,
    },
    Reset {
        revision: String,
        mode: ResetMode,
    },
    DiscardFiles(Vec<RawPath>),
    /// Replace a conflicted working file with the exact base/ours/theirs index object.
    /// This does not stage it; the user can inspect and stage the resolved contents separately.
    ResolveConflict {
        path: RawPath,
        stage: u8,
    },
    BisectStart {
        good: String,
        bad: String,
    },
    BisectMark {
        good: bool,
    },
    BisectSkip,
    BisectReset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceFingerprint(u64);
impl WorkspaceFingerprint {
    /// Bounded local-source continuity hint, never an approval or fresh check.
    pub fn navigation_identity(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }
}

#[derive(Debug, Clone)]
pub struct MutationPlan {
    pub operation: Operation,
    pub target: String,
    pub effect: String,
    pub destructive: bool,
    pub observed: WorkspaceFingerprint,
}

#[derive(Clone)]
pub struct GitRepository {
    pub root: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub bare: bool,
    cancellation: CancellationToken,
    navigation_read_window:
        Option<crate::providers::remote_git_read_window_v1::RemoteGitReadWindowV1>,
    retained_read_runner_v1: Option<CommandRunner>,
}

impl fmt::Debug for GitRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitRepository")
            .field("root", &self.root)
            .field("git_dir", &self.git_dir)
            .field("common_dir", &self.common_dir)
            .field("bare", &self.bare)
            .finish()
    }
}

impl GitRepository {
    pub fn discover(directory: impl AsRef<Path>) -> GitResult<Option<Self>> {
        Self::discover_with_cancellation(directory, CancellationToken::default())
    }

    pub fn discover_with_cancellation(
        directory: impl AsRef<Path>,
        cancellation: CancellationToken,
    ) -> GitResult<Option<Self>> {
        Self::discover_with_read_policy_v1(directory, cancellation, None)
    }

    fn discover_with_read_policy_v1(
        directory: impl AsRef<Path>,
        cancellation: CancellationToken,
        navigation_read_window: Option<
            crate::providers::remote_git_read_window_v1::RemoteGitReadWindowV1,
        >,
    ) -> GitResult<Option<Self>> {
        let directory = directory.as_ref();
        let output = run_git_with_window_v1(
            directory,
            ["rev-parse", "--absolute-git-dir"],
            None,
            false,
            &cancellation,
            navigation_read_window.as_ref(),
            None,
        )?;
        if !output.success {
            if output
                .stderr
                .windows(20)
                .any(|w| w == b"not a git repository")
            {
                return Ok(None);
            }
            return Err(output.error("Discover repository"));
        }
        let read = |directory: &Path, args: &[&str]| -> GitResult<Vec<u8>> {
            let result = run_git_with_window_v1(
                directory,
                args,
                None,
                false,
                &cancellation,
                navigation_read_window.as_ref(),
                None,
            )?;
            if result.success {
                Ok(result.stdout)
            } else {
                Err(result.error("Discover repository"))
            }
        };
        let bare_raw = read(directory, &["rev-parse", "--is-bare-repository"])?;
        let bare = trim_line(&bare_raw) == b"true";
        let root_raw = if bare {
            output.stdout.clone()
        } else {
            read(directory, &["rev-parse", "--show-toplevel"])?
        };
        let root = PathBuf::from(OsString::from_vec(trim_line(&root_raw).to_vec()));
        // A stale core.worktree (left by another tool) makes Git use an
        // unrelated directory as this checkout's working tree. Say so instead
        // of failing on the next read run from that directory.
        if !bare {
            let inside = std::fs::canonicalize(directory)
                .ok()
                .zip(std::fs::canonicalize(&root).ok())
                .is_none_or(|(here, root)| here.starts_with(root));
            if !inside {
                return Err(GitError(format!(
                    "This checkout's core.worktree points to {}, not this directory.\nGit uses that directory as the working tree (set in {}/config), so git status and ghtui read it instead.\nIf the setting is stale, run in this checkout:\n  git config --unset core.worktree",
                    root.display(),
                    String::from_utf8_lossy(trim_line(&output.stdout)),
                )));
            }
        }
        if let Some(window) = &navigation_read_window {
            window
                .observe_workflow_discovered_root_v9(&root, &cancellation)
                .map_err(|_| {
                    GitError("Original W9 discovery/environment root changed; no fallback".into())
                })?;
        }
        let common = read(
            &root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        let retained_runner = navigation_read_window
            .as_ref()
            .map(|window| window.actual_runner_v1().clone());
        Ok(Some(Self::discovered_from_bytes_v9(
            [&output.stdout, &bare_raw, &root_raw, &common],
            cancellation,
            navigation_read_window,
            retained_runner,
        )))
    }

    // Private SAME installed-Git discovery representation for ordinary live
    // discovery and W9's four fixed paid child outputs. No public raw source
    // constructor, saved-path factory or navigation-window promotion.
    fn discovered_from_bytes_v9(
        raw: [&[u8]; 4],
        cancellation: CancellationToken,
        navigation_read_window: Option<
            crate::providers::remote_git_read_window_v1::RemoteGitReadWindowV1,
        >,
        retained_read_runner_v1: Option<CommandRunner>,
    ) -> Self {
        Self {
            root: PathBuf::from(OsString::from_vec(trim_line(raw[2]).to_vec())),
            git_dir: PathBuf::from(OsString::from_vec(trim_line(raw[0]).to_vec())),
            common_dir: PathBuf::from(OsString::from_vec(trim_line(raw[3]).to_vec())),
            bare: trim_line(raw[1]) == b"true",
            cancellation,
            retained_read_runner_v1,
            navigation_read_window,
        }
    }

    /// Give each background request its own cancellation token; cancelled tokens are not reused.
    pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    pub(crate) fn actual_cancellation_v1(&self) -> &CancellationToken {
        &self.cancellation
    }
    pub(crate) fn with_navigation_read_window_v1(
        mut self,
        window: crate::providers::remote_git_read_window_v1::RemoteGitReadWindowV1,
    ) -> Self {
        self.navigation_read_window = Some(window);
        self
    }
    pub fn resolve(&self, revision: &str) -> GitResult<String> {
        reject_display_reference(revision)?;
        if revision.is_empty() || revision.starts_with('-') || revision.contains('\0') {
            return Err(GitError("Invalid revision".into()));
        }
        let revision = format!("{revision}^{{commit}}");
        self.read_text(&["rev-parse", "--verify", "--end-of-options", &revision])
    }

    /// Saved-view identity ignores replacement refs and grafts; mutations keep resolve semantics.
    pub fn resolve_original(&self, revision: &str) -> GitResult<String> {
        reject_display_reference(revision)?;
        if revision.is_empty() || revision.starts_with('-') || revision.contains('\0') {
            return Err(GitError("Invalid revision".into()));
        }
        self.original_text(&[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ])
    }

    /// Detect effective override configuration, including bare repos, without changing it.
    pub fn history_overrides(&self) -> GitResult<(usize, bool)> {
        let configured =
            std::env::var_os("GIT_REPLACE_REF_BASE").unwrap_or_else(|| "refs/replace".into());
        let prefix = replacement_prefix(&configured);
        Ok(self.history_override_flags_v9(&self.references()?, &prefix))
    }
    fn history_override_flags_v9(&self, references: &[Reference], prefix: &[u8]) -> (usize, bool) {
        let replacements = references
            .iter()
            .filter(|reference| reference.raw_name.starts_with(prefix))
            .count();
        let graft = std::env::var_os("GIT_GRAFT_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.common_dir.join("info/grafts"));
        let graft = if graft.is_relative() {
            self.root.join(graft)
        } else {
            graft
        };
        let grafted = match std::fs::metadata(graft) {
            Ok(metadata) => metadata.len() > 0,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => true,
        };
        (replacements, grafted)
    }

    pub fn status(&self) -> GitResult<RepoStatus> {
        if self.bare {
            return Err(GitError(
                "A bare repository has no working files or index".into(),
            ));
        }
        let bytes = self.read(&[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
        ])?;
        let mut status = parse_status(&bytes)?;
        status.operation = self.sequencer();
        status.shallow = self.read_text(&["rev-parse", "--is-shallow-repository"])? == "true";
        status.sparse = self
            .run(
                [
                    "config",
                    "--bool",
                    "--default=false",
                    "--get",
                    "core.sparseCheckout",
                ],
                None,
            )?
            .stdout
            == b"true\n";
        (status.replacement_refs, status.grafted) = self.history_overrides()?;
        Ok(status)
    }

    pub fn sequencer(&self) -> Option<Sequencer> {
        if self.git_dir.join("rebase-merge").exists() || self.git_dir.join("rebase-apply").exists()
        {
            Some(Sequencer::Rebase)
        } else if self.git_dir.join("MERGE_HEAD").exists() {
            Some(Sequencer::Merge)
        } else if self.git_dir.join("CHERRY_PICK_HEAD").exists() {
            Some(Sequencer::CherryPick)
        } else if self.git_dir.join("REVERT_HEAD").exists() {
            Some(Sequencer::Revert)
        } else if self.git_dir.join("BISECT_START").exists() {
            Some(Sequencer::Bisect)
        } else {
            None
        }
    }

    pub fn references(&self) -> GitResult<Vec<Reference>> {
        let bytes = self.read(&[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(upstream)%00%(HEAD)%00",
        ])?;
        Self::references_from_bytes_v9(&bytes)
    }
    fn references_from_bytes_v9(bytes: &[u8]) -> GitResult<Vec<Reference>> {
        bytes
            .split(|b| *b == b'\n')
            .filter(|row| !row.is_empty())
            .map(|row| {
                let fields = row.split(|b| *b == 0).collect::<Vec<_>>();
                if fields.len() < 4 {
                    return Err(GitError("Invalid reference record".into()));
                }
                Ok(Reference {
                    name: reference_label(fields[0]),
                    raw_name: fields[0].to_vec(),
                    id: text(fields[1]),
                    upstream: (!fields[2].is_empty()).then(|| reference_label(fields[2])),
                    raw_upstream: (!fields[2].is_empty()).then(|| fields[2].to_vec()),
                    current: fields[3] == b"*",
                })
            })
            .collect()
    }

    pub fn remotes(&self) -> GitResult<Vec<Remote>> {
        let names = self.read(&["remote"])?;
        names
            .split(|b| *b == b'\n')
            .filter(|name| !name.is_empty())
            .map(|name| {
                let name = text(name);
                Ok(Remote {
                    fetch_url: self.read_text(&["remote", "get-url", &name])?,
                    push_url: self.read_text(&["remote", "get-url", "--push", &name])?,
                    name,
                })
            })
            .collect()
    }

    pub fn log(&self, limit: usize) -> GitResult<Vec<Commit>> {
        self.log_page("HEAD", 0, limit)
    }

    pub fn log_page(&self, revision: &str, offset: usize, limit: usize) -> GitResult<Vec<Commit>> {
        self.log_page_with_capture_limit_v1(revision, offset, limit, MAX_OUTPUT)
    }
    pub(crate) fn log_page_with_capture_limit_v1(
        &self,
        revision: &str,
        offset: usize,
        limit: usize,
        capture_limit: usize,
    ) -> GitResult<Vec<Commit>> {
        if capture_limit == 0 || capture_limit > MAX_OUTPUT {
            return Err(GitError("Invalid original log capture limit".into()));
        }
        let revision = match self.resolve_original(revision) {
            Ok(revision) => revision,
            Err(_) if revision == "HEAD" && self.status()?.head.is_none() => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let limit = format!("--max-count={}", limit.min(10_000));
        let offset = format!("--skip={offset}");
        let bytes = self.original_bytes_with_capture_limit_v1(
            [
                "log",
                "--topo-order",
                &limit,
                &offset,
                "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%cn%x00%ce%x00%ct%x00%s%x00%D%x00",
                &revision,
                "--",
            ],
            None,
            capture_limit,
        )?;
        self.canonical_parents(parse_commits(&bytes)?)
    }

    pub fn tree(&self, revision: &str) -> GitResult<Vec<TreeEntry>> {
        let revision = self.resolve_original(revision)?;
        let bytes = self.original_bytes(
            ["ls-tree", "-r", "-t", "-z", "--full-tree", &revision],
            None,
        )?;
        parse_tree(&bytes)
    }

    /// Read one directory at a time. Returned paths are still relative to the repository root.
    pub fn tree_children(
        &self,
        revision: &str,
        directory: Option<&RawPath>,
    ) -> GitResult<Vec<TreeEntry>> {
        let revision = self.resolve_original(revision)?;
        let mut object = revision.into_bytes();
        if let Some(directory) = directory {
            validate_path(directory)?;
            object.push(b':');
            object.extend_from_slice(&directory.0);
        }
        let args = [
            OsString::from("ls-tree"),
            "-z".into(),
            "--full-tree".into(),
            OsString::from_vec(object),
        ];
        let mut entries = parse_tree(&self.original_bytes(args.iter(), None)?)?;
        if let Some(directory) = directory {
            for entry in &mut entries {
                let mut path = directory.0.clone();
                path.push(b'/');
                path.extend_from_slice(&entry.path.0);
                entry.path = RawPath(path);
            }
        }
        Ok(entries)
    }

    pub fn blob(&self, revision: &str, path: &RawPath) -> GitResult<Vec<u8>> {
        let entry = self.tree_entry(revision, path)?;
        if entry.kind != "blob" {
            return Err(GitError(
                "Selected object is a directory or submodule, not a file".into(),
            ));
        }
        self.object(&entry.id)
    }

    /// Resolve an exact path without scanning the complete repository tree.
    pub fn tree_entry(&self, revision: &str, path: &RawPath) -> GitResult<TreeEntry> {
        validate_path(path)?;
        let revision = self.resolve_original(revision)?;
        let args = [
            OsString::from("ls-tree"),
            "-z".into(),
            "--full-tree".into(),
            revision.into(),
            "--".into(),
            path.as_os_str().to_owned(),
        ];
        parse_tree(&self.original_bytes(args.iter(), None)?)?
            .into_iter()
            .find(|entry| entry.path == *path)
            .ok_or_else(|| {
                GitError(format!(
                    "{} does not exist in this saved version",
                    path.display()
                ))
            })
    }

    pub fn object(&self, id: &str) -> GitResult<Vec<u8>> {
        validate_object_id(id)?;
        self.original_bytes(["cat-file", "blob", id], None)
    }

    /// Batch immutable blob reads in one Git process; object IDs never drift with moving refs.
    pub fn objects(&self, ids: &[String]) -> GitResult<Vec<Vec<u8>>> {
        self.batch_objects(ids, "blob")
    }

    /// Inspect immutable blob sizes before allocating/reading their contents for search/indexing.
    pub fn object_sizes(&self, ids: &[String]) -> GitResult<Vec<usize>> {
        self.object_sizes_of_kind(ids, "blob")
    }

    /// Original immutable commit IDs only; no eager message/object allocation or checkout.
    pub fn commit_ids(
        &self,
        revision: &str,
        offset: usize,
        limit: usize,
    ) -> GitResult<Vec<String>> {
        if limit == 0 || limit > 10_000 {
            return Err(GitError("Commit collection limit must be 1..10000".into()));
        }
        let revision = self.resolve_original(revision)?;
        let limit = format!("--max-count={limit}");
        let offset = format!("--skip={offset}");
        let bytes = self.original_bytes(
            ["rev-list", "--topo-order", &limit, &offset, &revision, "--"],
            None,
        )?;
        bytes
            .split(|byte| *byte == b'\n')
            .filter(|row| !row.is_empty())
            .map(|row| {
                let id = String::from_utf8(row.to_vec())
                    .map_err(|_| GitError("Commit identity is not ASCII".into()))?;
                validate_object_id(&id)?;
                Ok(id)
            })
            .collect()
    }

    /// Saved-object readers need this without enumerating working files; valid for bare repos.
    pub fn is_shallow(&self) -> GitResult<bool> {
        Ok(self.original_text(&["rev-parse", "--is-shallow-repository"])? == "true")
    }

    /// Inspect sizes before bounded batched commit reads; replacements/grafts never rewrite them.
    pub fn commit_sizes(&self, ids: &[String]) -> GitResult<Vec<usize>> {
        self.object_sizes_of_kind(ids, "commit")
    }

    /// Raw original commit objects, including actual message bytes and signature headers.
    /// Callers must partition batches using commit_sizes to stay inside the shared output cap.
    pub fn commit_objects(&self, ids: &[String]) -> GitResult<Vec<Vec<u8>>> {
        self.batch_objects(ids, "commit")
    }

    fn object_sizes_of_kind(&self, ids: &[String], kind: &str) -> GitResult<Vec<usize>> {
        if ids.len() > 1024 {
            return Err(GitError("Limit one object size batch to 1024 files".into()));
        }
        let mut input = Vec::new();
        for id in ids {
            validate_object_id(id)?;
            input.extend_from_slice(id.as_bytes());
            input.push(b'\n');
        }
        let bytes = self.original_bytes(["cat-file", "--batch-check"], Some(&input))?;
        let records = bytes
            .split(|byte| *byte == b'\n')
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>();
        if records.len() != ids.len() {
            return Err(GitError("Truncated object size batch".into()));
        }
        records
            .iter()
            .zip(ids)
            .map(|(record, id)| {
                let fields = record.split(|byte| *byte == b' ').collect::<Vec<_>>();
                if fields.len() != 3 || fields[0] != id.as_bytes() || fields[1] != kind.as_bytes() {
                    return Err(GitError(format!(
                        "Object size requires an available immutable {kind}"
                    )));
                }
                text(fields[2])
                    .parse()
                    .map_err(|_| GitError("Invalid blob size".into()))
            })
            .collect()
    }

    fn batch_objects(&self, ids: &[String], kind: &str) -> GitResult<Vec<Vec<u8>>> {
        if ids.len() > 1024 {
            return Err(GitError("Limit one object batch to 1024 files".into()));
        }
        let mut input = Vec::new();
        for id in ids {
            validate_object_id(id)?;
            input.extend_from_slice(id.as_bytes());
            input.push(b'\n');
        }
        let bytes = self.original_bytes(["cat-file", "--batch"], Some(&input))?;
        let mut remaining = bytes.as_slice();
        let mut output = Vec::new();
        for id in ids {
            let split = remaining
                .iter()
                .position(|byte| *byte == b'\n')
                .ok_or_else(|| GitError("Truncated object batch header".into()))?;
            let fields = remaining[..split]
                .split(|byte| *byte == b' ')
                .collect::<Vec<_>>();
            if fields.len() != 3 || fields[0] != id.as_bytes() || fields[1] != kind.as_bytes() {
                return Err(GitError(format!(
                    "Object is unavailable or is not a {kind}"
                )));
            }
            let size: usize = text(fields[2])
                .parse()
                .map_err(|_| GitError("Invalid object size".into()))?;
            remaining = &remaining[split + 1..];
            if remaining.len() <= size || remaining[size] != b'\n' {
                return Err(GitError("Truncated object batch content".into()));
            }
            output.push(remaining[..size].to_vec());
            remaining = &remaining[size + 1..];
        }
        if !remaining.is_empty() {
            return Err(GitError("Extra object batch output".into()));
        }
        Ok(output)
    }

    pub fn ahead_behind(&self, base: &str, head: &str) -> GitResult<(usize, usize)> {
        let base = self.resolve_original(base)?;
        let head = self.resolve_original(head)?;
        let bytes = self.original_bytes(
            [
                "rev-list",
                "--left-right",
                "--count",
                &format!("{base}...{head}"),
            ],
            None,
        )?;
        let value = text(trim_line(&bytes));
        let mut counts = value.split_whitespace();
        let behind = counts
            .next()
            .unwrap_or_default()
            .parse()
            .map_err(|_| GitError("Invalid behind count".into()))?;
        let ahead = counts
            .next()
            .unwrap_or_default()
            .parse()
            .map_err(|_| GitError("Invalid ahead count".into()))?;
        Ok((ahead, behind))
    }

    /// Original-object ancestry, never replacement/graft history or implicit network fetching.
    /// Shallow counts describe available history only; multiple best bases are retained honestly.
    pub fn relationship(&self, base: &str, head: &str) -> GitResult<GitRelationship> {
        let base = self.resolve_original(base)?;
        let head = self.resolve_original(head)?;
        let (ahead, behind) = self.ahead_behind(&base, &head)?;
        let common = self.run(
            ["--no-replace-objects", "merge-base", "--all", &base, &head],
            None,
        )?;
        let common_ancestors = if common.success {
            let mut ids = common
                .stdout
                .split(|byte| *byte == b'\n')
                .filter(|row| !row.is_empty())
                .map(|row| {
                    let id = text(row);
                    validate_object_id(&id)?;
                    Ok(id)
                })
                .collect::<GitResult<Vec<_>>>()?;
            ids.sort();
            ids.dedup();
            ids
        } else if common.exit_code == Some(1) && common.stdout.is_empty() {
            // merge-base exits one for unrelated histories, including unavailable shallow bases.
            Vec::new()
        } else {
            return Err(common.error("Original saved-version ancestry"));
        };
        let shallow = self.original_text(&["rev-parse", "--is-shallow-repository"])? == "true";
        Ok(GitRelationship {
            base,
            head,
            ahead,
            behind,
            common_ancestors,
            shallow,
        })
    }

    pub fn compare(&self, comparison: &Comparison) -> GitResult<ComparisonResult> {
        self.compare_with_capture_limit_v1(comparison, MAX_OUTPUT)
    }
    pub(crate) fn compare_with_capture_limit_v1(
        &self,
        comparison: &Comparison,
        capture_limit: usize,
    ) -> GitResult<ComparisonResult> {
        if capture_limit == 0 || capture_limit > MAX_OUTPUT {
            return Err(GitError("Invalid original comparison capture limit".into()));
        }
        let observed = if matches!(
            comparison,
            Comparison::Working | Comparison::Uncommitted | Comparison::Staged
        ) {
            Some(self.fingerprint_with_capture_limit_v2(capture_limit)?)
        } else {
            None
        };
        let mut args = vec![
            "diff".to_owned(),
            "--no-ext-diff".into(),
            "--no-textconv".into(),
            "--no-color".into(),
            "--find-renames".into(),
        ];
        let (base, head, common_ancestor, description): (
            Option<String>,
            Option<String>,
            Option<String>,
            String,
        ) = match comparison {
            Comparison::Working => (
                None,
                None,
                None,
                "Unstaged edits compared to the next commit".into(),
            ),
            Comparison::Uncommitted => {
                let head = self.mutable_comparison_head_v2(capture_limit)?;
                if let Some(head) = &head {
                    args.push(head.clone());
                } else {
                    let empty_tree = self.bytes_with_capture_limit_v2(
                        ["hash-object", "-t", "tree", "--stdin"],
                        Some(&[]),
                        capture_limit,
                    )?;
                    args.push(text(trim_line(&empty_tree)));
                }
                (
                    head,
                    None,
                    None,
                    "All uncommitted tracked edits compared to the saved version".into(),
                )
            }
            Comparison::Staged => {
                args.push("--cached".into());
                let head = self.mutable_comparison_head_v2(capture_limit)?;
                if let Some(head) = &head {
                    args.push(head.clone());
                }
                (head, None, None, "Staged for the next commit".into())
            }
            Comparison::Snapshots { base, head } => {
                let base = self.resolve_original(base)?;
                let head = self.resolve_original(head)?;
                args.extend([base.clone(), head.clone()]);
                (
                    Some(base),
                    Some(head),
                    None,
                    "Compare two saved versions".into(),
                )
            }
            Comparison::BranchChanges { base, head } => {
                let base = self.resolve_original(base)?;
                let head = self.resolve_original(head)?;
                let common = self.original_text(&["merge-base", &base, &head]).map_err(|_|GitError("No common starting point is available. Histories may be unrelated or incomplete in a shallow clone.".into()))?;
                args.extend([common.clone(), head.clone()]);
                (
                    Some(base),
                    Some(head),
                    Some(common),
                    "Changes proposed by this branch since the common starting point".into(),
                )
            }
            Comparison::CommitChanges { commit, parent } => {
                let head = self.resolve_original(commit)?;
                let selected = self
                    .log_page_with_capture_limit_v1(&head, 0, 1, capture_limit)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| GitError("Selected saved commit is unavailable".into()))?;
                let (base, description) = if selected.parents.is_empty() {
                    if *parent != 0 {
                        return Err(GitError("A root commit has no parent; choose its starting content (parent zero)".into()));
                    }
                    let bytes =
                        self.original_bytes(["hash-object", "-t", "tree", "--stdin"], Some(&[]))?;
                    (
                        text(trim_line(&bytes)),
                        "Root starting content — all files introduced by this saved version".into(),
                    )
                } else {
                    let base = selected.parents.get(*parent).cloned().ok_or_else(|| {
                        GitError(format!(
                            "Selected commit has {} parent(s); requested parent {} is unavailable",
                            selected.parents.len(),
                            parent.saturating_add(1)
                        ))
                    })?;
                    let description = if *parent == 0 {
                        "Changes introduced by this saved version compared to its first parent"
                            .into()
                    } else {
                        format!("Changes introduced by this saved version compared to parent {} (explicit alternative to first parent)", parent.saturating_add(1))
                    };
                    (base, description)
                };
                args.extend([base.clone(), head.clone()]);
                (Some(base), Some(head), None, description)
            }
        };
        args.push("--".into());
        let original = matches!(
            comparison,
            Comparison::Snapshots { .. }
                | Comparison::BranchChanges { .. }
                | Comparison::CommitChanges { .. }
        );
        let patch = if original {
            self.original_bytes_with_capture_limit_v1(
                args.iter().map(OsStr::new),
                None,
                capture_limit,
            )?
        } else {
            self.bytes_with_capture_limit_v2(args.iter().map(OsStr::new), None, capture_limit)?
        };
        let mut diff = Diff::parse(&patch).map_err(GitError)?;
        let mut names_args = args.clone();
        names_args.insert(1, "--name-status".into());
        names_args.insert(2, "-z".into());
        let paths = if original {
            let names =
                self.original_bytes_with_capture_limit_v1(names_args.iter(), None, capture_limit)?;
            parse_diff_paths(&names)?
        } else {
            // Unlike name-status, numstat checks actual contents even with
            // automatic index refreshing disabled and external helpers denied.
            // It exits zero normally: failed runner stdout is never raw IDs.
            names_args[1] = "--numstat".into();
            let names = self.bytes_with_capture_limit_v2(names_args.iter(), None, capture_limit)?;
            readonly_diff_paths_v1::paths(&diff, &names, capture_limit)?
        };
        if diff.files.len() != paths.len() {
            return Err(GitError("Diff paths and patch disagree; conflict or combined diffs need the conflict viewer".into()));
        }
        for (file, (old, new)) in diff.files.iter_mut().zip(paths) {
            file.old_path = old;
            file.new_path = new;
        }
        diff.base = common_ancestor.clone().or_else(|| base.clone());
        diff.head = head.clone();
        if let Some(observed) = observed {
            if self.fingerprint_with_capture_limit_v2(capture_limit)? != observed {
                return Err(GitError(
                    "Repository changed while loading the diff. Refresh before selecting changes."
                        .into(),
                ));
            }
        }
        Ok(ComparisonResult {
            comparison: comparison.clone(),
            description,
            base,
            head,
            common_ancestor,
            diff,
            patch,
            observed,
        })
    }

    /// Preview a canonical line selection only against the state shown in its original diff.
    pub fn plan_selected_patch(
        &self,
        comparison: &ComparisonResult,
        file_index: usize,
        selected: &[(usize, usize)],
        reverse: bool,
    ) -> GitResult<MutationPlan> {
        if !matches!(
            (&comparison.comparison, reverse),
            (Comparison::Working, false) | (Comparison::Staged, true)
        ) {
            return Err(GitError(
                "Stage lines from Working diff, or unstage lines from Staged diff".into(),
            ));
        }
        let observed = comparison
            .observed
            .ok_or_else(|| GitError("A local diff snapshot is required".into()))?;
        if self.fingerprint()? != observed {
            return Err(GitError(
                "Selected diff is stale. Refresh and select changes again.".into(),
            ));
        }
        let patch = if reverse {
            comparison.diff.selected_unstage_patch(file_index, selected)
        } else {
            comparison.diff.selected_patch(file_index, selected)
        }
        .map_err(GitError)?;
        // Both direction-aware patches apply forward to the index snapshot actually shown.
        let mut plan = self.plan(Operation::ApplyPatch {
            patch,
            reverse: false,
        })?;
        // Keep the rendered snapshot identity even if an external writer races with plan preparation.
        plan.observed = observed;
        Ok(plan)
    }

    pub fn file_history(
        &self,
        revision: &str,
        path: &RawPath,
        limit: usize,
    ) -> GitResult<Vec<Commit>> {
        validate_path(path)?;
        let revision = self.resolve_original(revision)?;
        let args = [
            OsString::from("log"),
            OsString::from("--follow"),
            OsString::from(format!("--max-count={}", limit.min(10_000))),
            OsString::from(
                "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%cn%x00%ce%x00%ct%x00%s%x00%D%x00",
            ),
            OsString::from(revision),
            OsString::from("--"),
            path.as_os_str().to_owned(),
        ];
        self.canonical_parents(parse_commits(&self.original_bytes(args.iter(), None)?)?)
    }

    pub fn blame(&self, revision: &str, path: &RawPath) -> GitResult<Vec<BlameLine>> {
        validate_path(path)?;
        let revision = self.resolve_original(revision)?;
        let args = [
            OsString::from("blame"),
            OsString::from("--line-porcelain"),
            OsString::from(revision),
            OsString::from("--"),
            path.as_os_str().to_owned(),
        ];
        parse_blame(&self.original_bytes(args.iter(), None)?)
    }

    pub fn conflict_stages(&self) -> GitResult<Vec<ConflictStage>> {
        let bytes = self.read(&["ls-files", "--unmerged", "-z"])?;
        bytes
            .split(|b| *b == 0)
            .filter(|row| !row.is_empty())
            .map(|row| {
                let split = row
                    .iter()
                    .position(|b| *b == b'\t')
                    .ok_or_else(|| GitError("Invalid conflict record".into()))?;
                let fields = row[..split].split(|b| *b == b' ').collect::<Vec<_>>();
                if fields.len() != 3 {
                    return Err(GitError("Invalid conflict fields".into()));
                }
                Ok(ConflictStage {
                    mode: text(fields[0]),
                    id: text(fields[1]),
                    stage: text(fields[2])
                        .parse()
                        .map_err(|_| GitError("Invalid conflict stage".into()))?,
                    path: RawPath(row[split + 1..].to_vec()),
                })
            })
            .collect()
    }

    pub fn reflog(&self, limit: usize) -> GitResult<Vec<ReflogEntry>> {
        let limit = format!("--max-count={}", limit.min(10_000));
        let bytes = self.read(&["reflog", "show", &limit, "--format=%H%x00%gd%x00%gs%x00"])?;
        parse_three_fields(&bytes).map(|rows| {
            rows.into_iter()
                .map(|(id, selector, subject)| ReflogEntry {
                    id,
                    selector,
                    subject,
                })
                .collect()
        })
    }

    pub fn stashes(&self) -> GitResult<Vec<Stash>> {
        let bytes = self.read(&["stash", "list", "--format=%H%x00%gd%x00%gs%x00"])?;
        parse_three_fields(&bytes).map(|rows| {
            rows.into_iter()
                .map(|(id, selector, subject)| Stash {
                    id,
                    selector,
                    subject,
                })
                .collect()
        })
    }

    /// Overview-only stash summary: no child unless the stash reflog exists,
    /// and a read-only `reflog show` rather than any `stash` subcommand.
    pub fn stash_overview_v1(&self) -> GitResult<Vec<Stash>> {
        if !self.common_dir.join("logs/refs/stash").is_file() {
            return Ok(vec![]);
        }
        let bytes = self.read(&[
            "reflog",
            "show",
            "--max-count=80",
            "--format=%H%x00%gd%x00%gs%x00",
            "refs/stash",
            "--",
        ])?;
        parse_three_fields(&bytes).map(|rows| {
            rows.into_iter()
                .map(|(id, selector, subject)| Stash {
                    id,
                    selector,
                    subject,
                })
                .collect()
        })
    }

    /// Linked worktree metadata count from the common Git directory; no child.
    pub fn linked_worktree_count_v1(&self) -> usize {
        std::fs::read_dir(self.common_dir.join("worktrees"))
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or(0)
    }

    pub fn worktrees(&self) -> GitResult<Vec<Worktree>> {
        let bytes = self.read(&["worktree", "list", "--porcelain", "-z"])?;
        Ok(parse_worktrees_v9(&bytes))
    }

    pub fn fingerprint(&self) -> GitResult<WorkspaceFingerprint> {
        self.fingerprint_with_capture_limit_v2(MAX_OUTPUT)
    }

    /// Same continuity hint and runner as ordinary mutation previews, with a
    /// smaller native-view envelope. Filesystem bytes are bounded separately;
    /// they are not falsely charged as captured Git child output.
    pub(crate) fn fingerprint_with_capture_limit_v2(
        &self,
        cap: usize,
    ) -> GitResult<WorkspaceFingerprint> {
        self.fingerprint_and_config_v9(cap)
            .map(|(fingerprint, _)| fingerprint)
    }

    /// SAME five reads and SAME legacy fingerprint. W9 also retains actual
    /// bounded config records for hooks/signing/filter/remote continuity;
    /// ordinary callers still drop these bytes without a second child.
    fn fingerprint_and_config_v9(&self, cap: usize) -> GitResult<(WorkspaceFingerprint, Vec<u8>)> {
        if cap == 0 || cap > MAX_OUTPUT {
            return Err(GitError("Invalid mutable fingerprint capture limit".into()));
        }
        self.check_mutable_read_v2()?;
        let mut hash = DefaultHasher::new();
        let status = self.bytes_with_capture_limit_v2(
            [
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
            ],
            None,
            cap,
        )?;
        status.hash(&mut hash);
        self.bytes_with_capture_limit_v2(["ls-files", "--stage", "-z"], None, cap)?
            .hash(&mut hash);
        self.bytes_with_capture_limit_v2(
            [
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(upstream)%00",
            ],
            None,
            cap,
        )?
        .hash(&mut hash);
        // An absent remote is normal local state. --get-regexp exits one for
        // no match; repeating that probe would consume two full failed-read
        // reservations inside a correctly measured operation. Enumerate once
        // and retain exactly the same raw remote key/value records instead.
        let config = self.bytes_with_capture_limit_v2(["config", "--null", "--list"], None, cap)?;
        self.hash_remote_config_v9(&config, &mut hash)?;
        self.bytes_with_capture_limit_v2(
            ["diff", "--no-ext-diff", "--no-textconv", "--binary"],
            None,
            cap,
        )?
        .hash(&mut hash);
        self.finish_fingerprint_v9(cap, hash, &status, config, None)
    }
    fn hash_remote_config_v9(&self, config: &[u8], hash: &mut DefaultHasher) -> GitResult<()> {
        let mut remote_config = Vec::new();
        for record in config
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
        {
            self.check_mutable_read_v2()?;
            let key = record
                .split(|byte| *byte == b'\n')
                .next()
                .unwrap_or_default();
            if key.starts_with(b"remote.") {
                remote_config.extend_from_slice(record);
                remote_config.push(0);
            }
        }
        remote_config.hash(hash);
        Ok(())
    }
    fn finish_fingerprint_v9(
        &self,
        cap: usize,
        mut hash: DefaultHasher,
        status: &[u8],
        config: Vec<u8>,
        deadline: Option<std::time::Instant>,
    ) -> GitResult<(WorkspaceFingerprint, Vec<u8>)> {
        let check = || {
            self.check_mutable_read_v2()?;
            if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
                return Err(GitError(
                    "Original workflow post fingerprint expired; no renewed observation".into(),
                ));
            }
            Ok(())
        };
        let mut untracked_bytes = 0u64;
        for entry in parse_status(status)?
            .entries
            .into_iter()
            .filter(StatusEntry::untracked)
        {
            check()?;
            let path = self.root.join(entry.path.as_os_str());
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                std::fs::read_link(path)?.hash(&mut hash);
            } else if metadata.is_file() {
                if metadata.len() > cap as u64 {
                    return Err(GitError(
                        "Untracked file exceeds safe mutation preview size".into(),
                    ));
                }
                untracked_bytes = untracked_bytes.saturating_add(metadata.len());
                if untracked_bytes > cap as u64 {
                    return Err(GitError(format!("Untracked content exceeds the safe mutation preview byte budget ({}MiB); ignore or move unrelated data before previewing a write", cap / (1024 * 1024))));
                }
                let mut file = std::fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                    .open(path)?;
                let opened = file.metadata()?;
                if !opened.is_file() || opened.len() != metadata.len() {
                    return Err(GitError(
                        "Untracked file changed during mutation preview; refresh first".into(),
                    ));
                }
                opened.len().hash(&mut hash);
                let mut count = 0u64;
                let mut buffer = [0u8; 64 * 1024];
                loop {
                    check()?;
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    count = count.saturating_add(read as u64);
                    if count > opened.len() {
                        return Err(GitError(
                            "Untracked file grew during mutation preview; refresh first".into(),
                        ));
                    }
                    hash.write(&buffer[..read]);
                }
                if count != opened.len() {
                    return Err(GitError(
                        "Untracked file changed during mutation preview; refresh first".into(),
                    ));
                }
            }
        }
        check()?;
        format!("{:?}", self.sequencer()).hash(&mut hash);
        let result = WorkspaceFingerprint(hash.finish());
        check()?;
        Ok((result, config))
    }

    fn check_mutable_read_v2(&self) -> GitResult<()> {
        if self.cancellation.is_cancelled() {
            return Err(GitError("Git mutation preview cancelled".into()));
        }
        if let Some(window) = &self.navigation_read_window {
            window
                .check(&self.cancellation)
                .map_err(|error| GitError(error.to_string()))?;
        }
        Ok(())
    }

    fn mutable_comparison_head_v2(&self, cap: usize) -> GitResult<Option<String>> {
        // Preserve the exact legacy status workflow; the native producer needs
        // only the same parser's observed HEAD, not unrelated decorations.
        if cap == MAX_OUTPUT {
            return Ok(self.status()?.head);
        }
        let bytes = self.bytes_with_capture_limit_v2(
            [
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
            ],
            None,
            cap,
        )?;
        Ok(parse_status(&bytes)?.head)
    }

    /// Refresh and acknowledge local state after a failed write before planning another operation.
    /// The caller must present this state to the user; a failed command may have produced a commit,
    /// conflicts, staged changes, or remote effects. Remote push outcomes need remote-ref inspection.
    pub fn reconcile_write(&self) -> GitResult<RepoStatus> {
        let status = self.status()?;
        self.fingerprint()?;
        RUNNER
            .get_or_init(CommandRunner::default)
            .mark_reconciled(&format!("git:{}", self.root.display()));
        Ok(status)
    }

    /// A mutation is always first converted into a target/effect preview with an observed state.
    /// Read-only preview lines naming the exact commits (full SHA + subject) a
    /// history operation would apply, replay, revert, skip or move off the
    /// branch, plus the exact local changes a hard/mixed reset would drop.
    pub fn affected_preview_v1(
        &self,
        operation: &Operation,
        status: &RepoStatus,
    ) -> GitResult<Vec<String>> {
        const SHOWN: usize = 50;
        let list = |title: &str, range: &str, no_merges: bool| -> GitResult<Vec<String>> {
            let count = format!("--max-count={}", SHOWN + 1);
            let mut args = vec!["log", "--no-color", "--format=%H %s", count.as_str()];
            if no_merges {
                args.push("--no-merges");
            }
            args.extend([range, "--"]);
            let bytes = self.read(&args)?;
            let mut rows = bytes
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| format!("  {}", display_bytes(line)))
                .collect::<Vec<_>>();
            let more = rows.len() > SHOWN;
            rows.truncate(SHOWN);
            let mut lines = vec![format!(
                "{title} ({}{}):",
                rows.len(),
                if more { "+" } else { "" }
            )];
            if rows.is_empty() {
                lines.push("  none".into());
            }
            lines.extend(rows);
            if more {
                lines.push(format!("  … list stops after {SHOWN} commits"));
            }
            Ok(lines)
        };
        let head = || self.resolve("HEAD");
        let changed = |hard: bool| {
            status
                .entries
                .iter()
                .filter(|entry| !entry.untracked())
                .filter(|entry| entry.staged() || entry.conflict || (hard && entry.worktree != '.'))
                .map(|entry| format!("  {}", entry.path.display()))
                .collect::<Vec<_>>()
        };
        match operation {
            Operation::Merge(revision) => {
                let id = self.resolve(revision)?;
                list(
                    "Commits this merge brings in",
                    &format!("{}..{id}", head()?),
                    false,
                )
            }
            Operation::Rebase(revision) => {
                let id = self.resolve(revision)?;
                list(
                    &format!("Commits replayed onto {id}"),
                    &format!("{id}..{}", head()?),
                    true,
                )
            }
            Operation::CherryPick(revision) => {
                let id = self.resolve(revision)?;
                list("Commit applied on top of HEAD", &format!("{id}^!"), false)
            }
            Operation::Revert(revision) => {
                let id = self.resolve(revision)?;
                list("Commit whose changes are undone", &format!("{id}^!"), false)
            }
            Operation::Continue | Operation::Skip | Operation::Abort => {
                let name = match status.operation {
                    Some(Sequencer::Merge) => "MERGE_HEAD",
                    Some(Sequencer::Rebase) => "REBASE_HEAD",
                    Some(Sequencer::CherryPick) => "CHERRY_PICK_HEAD",
                    Some(Sequencer::Revert) => "REVERT_HEAD",
                    _ => return Ok(Vec::new()),
                };
                let Ok(raw) = std::fs::read(self.git_dir.join(name)) else {
                    return Ok(Vec::new());
                };
                let id = display_bytes(raw.split(|byte| *byte == b'\n').next().unwrap_or(&[]));
                if id.len() < 40 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Ok(Vec::new());
                }
                let title = match operation {
                    Operation::Continue => "Stopped commit that continues with the resolved index",
                    Operation::Skip => "Stopped commit that is skipped (its changes are dropped)",
                    _ => "Stopped commit; abort returns to the state before the operation",
                };
                list(title, &format!("{id}^!"), false)
            }
            Operation::Reset { revision, mode } => {
                let id = self.resolve(revision)?;
                let mut lines = list(
                    "Commits leaving the current branch (reflog keeps them until it expires)",
                    &format!("{id}..{}", head()?),
                    false,
                )?;
                let (title, paths) = match mode {
                    ResetMode::Hard => ("Uncommitted tracked changes discarded", changed(true)),
                    ResetMode::Mixed => (
                        "Staged changes unstaged; working files kept",
                        changed(false),
                    ),
                    ResetMode::Soft => ("Index and working files are kept", Vec::new()),
                };
                lines.push(format!("{title} ({}):", paths.len()));
                if paths.is_empty() {
                    lines.push("  none".into());
                }
                lines.extend(paths);
                if *mode == ResetMode::Hard {
                    lines.push("Untracked files are kept.".into());
                }
                Ok(lines)
            }
            Operation::Commit { amend: true, .. } => {
                // An unborn branch has nothing to amend; Git itself refuses it.
                let Ok(id) = head() else {
                    return Ok(Vec::new());
                };
                let mut lines = list(
                    "Commit replaced by this amend; the reflog keeps it until it expires",
                    &format!("{id}^!"),
                    false,
                )?;
                if let Some(upstream) = &status.upstream {
                    if status.ahead == 0 {
                        lines.push(format!(
                            "Warning: this commit is already on {upstream}; amending rewrites published history and the next push needs force-with-lease."
                        ));
                    }
                }
                Ok(lines)
            }
            _ => Ok(Vec::new()),
        }
    }

    pub fn plan(&self, operation: Operation) -> GitResult<MutationPlan> {
        self.plan_with_config_v9(operation, None)
    }
    pub(crate) fn plan_workflow_edit_v9(
        &self,
        purpose: &workflow_edit_v9::WorkflowPlanPurposeV9,
    ) -> GitResult<MutationPlan> {
        purpose.validate(self)?;
        self.plan_with_config_v9(purpose.operation(), Some(purpose.original_config_v9()))
    }
    pub(crate) fn plan_workflow_worktree_v9(
        &self,
        purpose: &workflow_edit_v9::WorkflowWorktreePurposeV9,
    ) -> GitResult<MutationPlan> {
        purpose.validate(self)?;
        self.plan_with_config_v9(purpose.operation(), Some(purpose.original_config()))
    }
    fn plan_with_config_v9(
        &self,
        operation: Operation,
        original_config: Option<&[u8]>,
    ) -> GitResult<MutationPlan> {
        let status = self.status()?;
        let args = self.operation_args(&operation, &status)?;
        let target = match &operation {
            Operation::Fetch { remote }
            | Operation::Pull { remote, .. }
            | Operation::PullWithStrategy { remote, .. } => {
                let selected = self
                    .remotes()?
                    .into_iter()
                    .find(|item| item.name == *remote)
                    .ok_or_else(|| GitError("Remote is unavailable".into()))?;
                format!(
                    "{} · {} ({})",
                    self.root.display(),
                    remote,
                    display_remote_url(&selected.fetch_url)
                )
            }
            Operation::Push { remote, .. } | Operation::ForcePushLease { remote, .. } => {
                let selected = self
                    .remotes()?
                    .into_iter()
                    .find(|item| item.name == *remote)
                    .ok_or_else(|| GitError("Remote is unavailable".into()))?;
                format!(
                    "{} · {} ({})",
                    self.root.display(),
                    remote,
                    display_remote_url(&selected.push_url)
                )
            }
            _ => self.root.display().to_string(),
        };
        let effect = args
            .iter()
            .map(|arg| display_bytes(arg.as_bytes()))
            .collect::<Vec<_>>()
            .join(" ");
        let destructive = matches!(
            operation,
            Operation::DeleteBranch { .. }
                | Operation::DeleteTag(_)
                | Operation::StashDrop { .. }
                | Operation::StashApply { pop: true, .. }
                | Operation::RemoveWorktree(_)
                | Operation::Commit { amend: true, .. }
                | Operation::ForcePushLease { .. }
                | Operation::Reset { .. }
                | Operation::DiscardFiles(_)
                | Operation::ResolveConflict { .. }
        );
        // SAME five existing fingerprint captures. Legacy None does not
        // compare additional configuration or issue any additional child.
        let (observed, config) = self.fingerprint_and_config_v9(if original_config.is_some() {
            256 * 1024
        } else {
            MAX_OUTPUT
        })?;
        if original_config.is_some_and(|expected| expected != config) {
            return Err(GitError(
                "Original workflow Git configuration changed; inspect a fresh explicit local plan"
                    .into(),
            ));
        }
        Ok(MutationPlan {
            operation,
            target,
            effect,
            destructive,
            observed,
        })
    }

    /// Call only after the UI's explicit target/effect confirmation. Reject stale previews.
    pub fn execute(&self, plan: &MutationPlan, confirmed: bool) -> GitResult<String> {
        self.execute_checked_v2(plan, confirmed, None)
    }
    pub(crate) fn execute_native_selected_v2(
        &self,
        purpose: &crate::repository_native_mutable_plan_v2::NativeMutableConfirmRequestV2,
        operation: &crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    ) -> GitResult<String> {
        operation.check().map_err(GitError)?;
        let original = purpose.source().intent().origin().original_repository_v1();
        if self.root != original.root
            || self.git_dir != original.git_dir
            || self.common_dir != original.common_dir
            || !self
                .actual_read_runner_v1()
                .same_instance(original.actual_read_runner_v1())
            || !self
                .navigation_read_window
                .as_ref()
                .is_some_and(|window| window.same_operation_v2(&operation.window()))
        {
            return Err(GitError("Confirmed selected purpose does not own the exact original repository/runner/read operation".into()));
        }
        if !matches!(&purpose.plan().mutation.operation,Operation::ApplyPatch{patch,reverse:false}if !patch.is_empty()&&patch.len()<=2*1024*1024)
        {
            return Err(GitError(
                "Native confirmation accepts only its exact bounded selected ApplyPatch".into(),
            ));
        }
        purpose
            .bind_original_operation_v2(operation)
            .map_err(GitError)?;
        purpose.reserve_attempt().map_err(GitError)?;
        self.execute_checked_v2(&purpose.plan().mutation, true, Some((purpose, operation)))
    }
    fn execute_checked_v2(
        &self,
        plan: &MutationPlan,
        confirmed: bool,
        native: Option<NativeSelectedExecutionV2<'_>>,
    ) -> GitResult<String> {
        if self.navigation_read_window.is_some() && native.is_none() {
            return Err(GitError(
                "Read-only remote navigation cannot execute local mutations".into(),
            ));
        }
        if !confirmed {
            return Err(GitError("Operation was not confirmed".into()));
        }
        let map = MUTATION_LOCKS.get_or_init(Mutex::default);
        let mut map_guard = if let Some((_, operation)) = native {
            loop {
                operation.check().map_err(GitError)?;
                match map.try_lock() {
                    Ok(guard) => break guard,
                    Err(std::sync::TryLockError::Poisoned(error)) => break error.into_inner(),
                    Err(std::sync::TryLockError::WouldBlock) => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                }
            }
        } else {
            map.lock().unwrap_or_else(|error| error.into_inner())
        };
        let lock = map_guard
            .entry(self.common_dir.clone())
            .or_default()
            .clone();
        drop(map_guard);
        let _write_guard = if let Some((purpose, operation)) = native {
            if !std::ptr::eq(plan, &purpose.plan().mutation) {
                return Err(GitError("Confirmed selected plan pointer changed".into()));
            }
            loop {
                operation.check().map_err(GitError)?;
                match lock.try_lock() {
                    Ok(guard) => break guard,
                    Err(std::sync::TryLockError::Poisoned(error)) => break error.into_inner(),
                    Err(std::sync::TryLockError::WouldBlock) => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                }
            }
        } else {
            lock.lock().unwrap_or_else(|error| error.into_inner())
        };
        if let Some((purpose, operation)) = native {
            operation.check().map_err(GitError)?;
            purpose
                .source()
                .validate_current(operation)
                .map_err(GitError)?;
        }
        if self.fingerprint()? != plan.observed {
            return Err(GitError(
                "Repository changed since the preview. Refresh and confirm a new preview.".into(),
            ));
        }
        let status = self.status()?;
        let args = self.operation_args(&plan.operation, &status)?;
        let stdin = match &plan.operation {
            Operation::ApplyPatch { patch, .. } => Some(patch.as_slice()),
            Operation::Commit { message, .. } => Some(message.as_bytes()),
            _ => None,
        };
        let result = if let Some((_, operation)) = native {
            operation.check().map_err(GitError)?;
            run_git_bounded_policy_deadline_v2(
                &self.root,
                args.iter(),
                stdin,
                true,
                &self.cancellation,
                GitReadPolicyV1 {
                    timeout: COMMAND_TIMEOUT.min(
                        operation
                            .deadline()
                            .saturating_duration_since(std::time::Instant::now()),
                    ),
                    output_limit: 2 * 1024 * 1024,
                    window: None,
                    runner: Some(self.actual_read_runner_v1()),
                },
                Some(operation.deadline()),
            )?
        } else {
            run_git_with_kind(&self.root, args.iter(), stdin, true, &self.cancellation)?
        };
        if !result.success {
            return Err(result.error("Git operation"));
        }
        // Operation progress may include hooks/remote output. Never apply this redaction to
        // original blobs or diffs, where exact user content must remain untouched.
        let mut output = git_diagnostic(&result.stdout);
        let warnings = git_diagnostic(&result.stderr);
        if !warnings.is_empty() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&warnings);
        }
        Ok(output)
    }

    fn operation_args(
        &self,
        operation: &Operation,
        status: &RepoStatus,
    ) -> GitResult<Vec<OsString>> {
        let mut args: Vec<OsString> = Vec::new();
        if status.has_history_overrides()
            && !matches!(
                operation,
                Operation::Stage(_)
                    | Operation::Unstage(_)
                    | Operation::ApplyPatch { .. }
                    | Operation::Fetch { .. }
            )
        {
            return Err(GitError("History writes are guarded while replacement refs or legacy grafts are active. Remove/disable those overrides explicitly before history changes; staging keeps installed-Git semantics.".into()));
        }
        let mut add = |values: &[&str]| {
            args.extend(values.iter().map(OsString::from));
        };
        let clean_required = matches!(
            operation,
            Operation::SwitchBranch(_)
                | Operation::Pull { .. }
                | Operation::PullWithStrategy { .. }
                | Operation::Merge(_)
                | Operation::Rebase(_)
                | Operation::CherryPick(_)
                | Operation::Revert(_)
                | Operation::BisectStart { .. }
        );
        if clean_required && !status.clean() {
            return Err(GitError(
                "Save or stash local changes before changing history or branches".into(),
            ));
        }
        if status.operation.is_some()
            && !matches!(
                operation,
                Operation::Stage(_)
                    | Operation::Unstage(_)
                    | Operation::ApplyPatch { .. }
                    | Operation::ResolveConflict { .. }
                    | Operation::Continue
                    | Operation::Skip
                    | Operation::Abort
                    | Operation::BisectMark { .. }
                    | Operation::BisectSkip
                    | Operation::BisectReset
            )
        {
            return Err(GitError(
                "Finish or abort the current Git operation first".into(),
            ));
        }
        match operation {
            Operation::Stage(paths) => {
                if paths.is_empty() {
                    return Err(GitError("Select files to stage".into()));
                }
                add(&["add", "--"]);
                for path in paths {
                    validate_path(path)?;
                    match std::fs::symlink_metadata(self.root.join(path.as_os_str())) {
                        Ok(metadata) if metadata.is_dir() => return Err(GitError("Directory staging is refused: select exact files so their bytes can be verified in the preview.".into())),
                        Ok(metadata) if !metadata.is_file() && !metadata.file_type().is_symlink() => return Err(GitError("Only regular files, symbolic links, or tracked deletions can be staged through an exact-file preview.".into())),
                        Ok(_) => {},
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                        Err(error) => return Err(error.into()),
                    }
                    args.push(path.as_os_str().to_owned());
                }
            }
            Operation::Unstage(paths) => {
                if paths.is_empty() {
                    return Err(GitError("Select files to unstage".into()));
                }
                if status.head.is_none() {
                    add(&["rm", "--cached", "--"]);
                } else {
                    add(&["restore", "--staged", "--"]);
                }
                let mut restored = Vec::<&RawPath>::new();
                for path in paths {
                    validate_path(path)?;
                    if !restored.contains(&path) {
                        restored.push(path);
                    }
                    // Unstaging one side of a staged rename restores both paths, so the
                    // index never keeps a lone deletion of the original name.
                    if let Some(original) = status
                        .entries
                        .iter()
                        .find(|entry| entry.index == 'R' && entry.path == *path)
                        .and_then(|entry| entry.original_path.as_ref())
                    {
                        validate_path(original)?;
                        if !restored.contains(&original) {
                            restored.push(original);
                        }
                    }
                }
                for path in restored {
                    args.push(path.as_os_str().to_owned());
                }
            }
            Operation::ApplyPatch { reverse, .. } => {
                add(&["apply", "--cached", "--recount", "--whitespace=nowarn"]);
                if *reverse {
                    args.push("--reverse".into());
                }
                args.push("-".into());
            }
            Operation::Commit { message, amend } => {
                if message.trim().is_empty() {
                    return Err(GitError("A commit message is required".into()));
                }
                if status.conflicts() > 0 {
                    return Err(GitError(
                        "Resolve and stage all conflicts before committing".into(),
                    ));
                }
                add(&["commit", "--file=-"]);
                if *amend {
                    args.push("--amend".into());
                }
            }
            Operation::CreateBranch { name, revision }
            | Operation::RecoverBranch { name, revision } => {
                self.validate_ref(name)?;
                let id = self.resolve(revision)?;
                add(&["branch", "--", name, &id]);
            }
            Operation::RenameBranch { old, new } => {
                self.validate_ref(old)?;
                self.validate_ref(new)?;
                add(&["branch", "-m", "--", old, new]);
            }
            Operation::SwitchBranch(name) => {
                self.validate_ref(name)?;
                add(&["switch", "--", name]);
            }
            Operation::DeleteBranch { name, force } => {
                self.validate_ref(name)?;
                add(&["branch", if *force { "-D" } else { "-d" }, "--", name]);
            }
            Operation::CreateTag { name, revision } => {
                self.validate_ref(name)?;
                let id = self.resolve(revision)?;
                add(&["tag", "--", name, &id]);
            }
            Operation::DeleteTag(name) => {
                self.validate_ref(name)?;
                add(&["tag", "-d", "--", name]);
            }
            Operation::SetUpstream { branch, upstream } => {
                self.validate_ref(branch)?;
                self.resolve(upstream)?;
                add(&[
                    "branch",
                    &format!("--set-upstream-to={upstream}"),
                    "--",
                    branch,
                ]);
            }
            Operation::Fetch { remote } => {
                self.validate_remote(remote)?;
                add(&["fetch", "--", remote]);
            }
            Operation::Pull { remote, branch } => {
                self.validate_remote(remote)?;
                self.validate_ref(branch)?;
                add(&["pull", "--ff-only", "--", remote, branch]);
            }
            Operation::PullWithStrategy {
                remote,
                branch,
                strategy,
            } => {
                self.validate_remote(remote)?;
                self.validate_ref(branch)?;
                add(&[
                    "pull",
                    match strategy {
                        PullStrategy::FastForwardOnly => "--ff-only",
                        PullStrategy::Merge => "--no-rebase",
                        PullStrategy::Rebase => "--rebase",
                    },
                    "--no-edit",
                    "--",
                    remote,
                    branch,
                ]);
            }
            Operation::Push {
                remote,
                branch,
                set_upstream,
            } => {
                self.validate_remote(remote)?;
                self.validate_ref(branch)?;
                add(&["push"]);
                if *set_upstream {
                    args.push("--set-upstream".into());
                }
                args.extend([
                    "--".into(),
                    remote.into(),
                    format!("refs/heads/{branch}:refs/heads/{branch}").into(),
                ]);
            }
            Operation::StashSave { message, untracked } => {
                add(&["stash", "push", "--message", message]);
                if *untracked {
                    // Under --literal-pathspecs Git's own cleanup pathspec (":/") is
                    // literal, so saved untracked files stayed in place. An explicit
                    // root pathspec (cwd is the worktree root) removes them as Git
                    // intends once they are safely in the stash.
                    args.extend(["--include-untracked", "--", "."].map(OsString::from));
                }
            }
            Operation::ForcePushLease {
                remote,
                branch,
                expected_remote,
            } => {
                self.validate_remote(remote)?;
                self.validate_ref(branch)?;
                validate_object_id(expected_remote)?;
                let push_url = self
                    .remotes()?
                    .into_iter()
                    .find(|item| item.name == *remote)
                    .ok_or_else(|| GitError("Remote is unavailable".into()))?
                    .push_url;
                let current = self.read(&[
                    "ls-remote",
                    "--heads",
                    "--",
                    &push_url,
                    &format!("refs/heads/{branch}"),
                ])?;
                let current = text(&current);
                let identity = current.split_whitespace().next().ok_or_else(|| {
                    GitError("Remote branch does not exist; refresh its identity".into())
                })?;
                if identity != expected_remote {
                    return Err(GitError("Remote branch changed since it was inspected. Fetch and review the new commits before force pushing.".into()));
                }
                add(&[
                    "push",
                    &format!("--force-with-lease=refs/heads/{branch}:{expected_remote}"),
                    "--",
                    remote,
                    &format!("refs/heads/{branch}:refs/heads/{branch}"),
                ]);
            }
            Operation::StashApply { id, pop } => {
                let stash = self.resolve_stash(id)?;
                add(&[
                    "stash",
                    if *pop { "pop" } else { "apply" },
                    "--",
                    &stash.selector,
                ]);
            }
            Operation::StashDrop { id } => {
                let stash = self.resolve_stash(id)?;
                add(&["stash", "drop", "--", &stash.selector]);
            }
            Operation::AddWorktree { path, branch } => {
                self.validate_ref(branch)?;
                if !path.is_absolute() {
                    return Err(GitError("Worktree target must be an absolute path".into()));
                }
                add(&["worktree", "add", "--"]);
                args.push(path.as_os_str().to_owned());
                args.push(branch.into());
            }
            Operation::AddWorktreeBranchAtV9 {
                path,
                branch,
                commit,
            } => {
                self.validate_ref(branch)?;
                if !path.is_absolute()
                    || !original_actor_saved_sha(commit)
                    || !matches!(std::fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
                {
                    return Err(GitError(
                        "New worktree needs an absent absolute target and exact saved commit"
                            .into(),
                    ));
                }
                let exact_branch = format!("refs/heads/{branch}");
                if self
                    .references()?
                    .iter()
                    .any(|reference| reference.raw_name == exact_branch.as_bytes())
                {
                    return Err(GitError(
                        "New worktree branch already exists; no replacement or fallback".into(),
                    ));
                }
                add(&["worktree", "add", "-b", branch, "--"]);
                args.push(path.as_os_str().to_owned());
                args.push(commit.into());
            }
            Operation::RemoveWorktree(path) => {
                let path = path.canonicalize()?;
                if path == self.root {
                    return Err(GitError("The current worktree cannot remove itself".into()));
                }
                let listed = self.worktrees()?;
                let Some(tree) = listed.iter().find(|tree| tree.path == path) else {
                    return Err(GitError("Target is not a linked worktree".into()));
                };
                if tree.locked {
                    return Err(GitError("Worktree removal refused: the worktree is locked. Inspect and unlock it explicitly (git worktree unlock) first.".into()));
                }
                let ignored = run_git_with_kind(
                    &path,
                    [
                        "ls-files",
                        "--others",
                        "--ignored",
                        "--exclude-standard",
                        "-z",
                    ],
                    None,
                    false,
                    &self.cancellation,
                )?;
                if !ignored.success {
                    return Err(ignored.error("Inspect ignored worktree files"));
                }
                if !ignored.stdout.is_empty() {
                    return Err(GitError("Worktree removal refused: ignored local files would be deleted. Move or remove those files explicitly first.".into()));
                }
                add(&["worktree", "remove", "--"]);
                args.push(path.into_os_string());
            }
            Operation::Merge(revision) => {
                let id = self.resolve(revision)?;
                add(&["merge", "--no-edit", "--", &id]);
            }
            Operation::Rebase(revision) => {
                let id = self.resolve(revision)?;
                add(&["rebase", "--", &id]);
            }
            Operation::CherryPick(revision) => {
                let id = self.resolve(revision)?;
                add(&["cherry-pick", "--", &id]);
            }
            Operation::Revert(revision) => {
                let id = self.resolve(revision)?;
                add(&["revert", "--no-edit", "--", &id]);
            }
            Operation::Reset { revision, mode } => {
                let id = self.resolve(revision)?;
                add(&[
                    "reset",
                    match mode {
                        ResetMode::Soft => "--soft",
                        ResetMode::Mixed => "--mixed",
                        ResetMode::Hard => "--hard",
                    },
                    &id,
                    "--",
                ]);
            }
            Operation::DiscardFiles(paths) => {
                if paths.is_empty() {
                    return Err(GitError("Select exact tracked files to discard".into()));
                }
                self.resolve("HEAD")?;
                add(&["restore", "--worktree", "--source=HEAD", "--"]);
                for path in paths {
                    validate_path(path)?;
                    self.tree_entry("HEAD", path)?;
                    args.push(path.as_os_str().to_owned());
                }
            }
            Operation::ResolveConflict { path, stage } => {
                validate_path(path)?;
                if !matches!(stage, 1..=3)
                    || !self
                        .conflict_stages()?
                        .iter()
                        .any(|entry| entry.path == *path && entry.stage == *stage)
                {
                    return Err(GitError(
                        "Selected conflict stage is unavailable for this exact path".into(),
                    ));
                }
                add(&[
                    "checkout-index",
                    "--force",
                    &format!("--stage={stage}"),
                    "--",
                ]);
                args.push(path.as_os_str().to_owned());
            }
            Operation::Continue | Operation::Skip | Operation::Abort => {
                let sequencer = status
                    .operation
                    .as_ref()
                    .ok_or_else(|| GitError("No Git operation is active".into()))?;
                let command = match sequencer {
                    Sequencer::Merge => "merge",
                    Sequencer::Rebase => "rebase",
                    Sequencer::CherryPick => "cherry-pick",
                    Sequencer::Revert => "revert",
                    Sequencer::Bisect => {
                        return Err(GitError(
                            "Use bisect reset to finish the investigation".into(),
                        ))
                    }
                };
                let flag = match operation {
                    Operation::Continue => "--continue",
                    Operation::Skip => "--skip",
                    _ => "--abort",
                };
                if *sequencer == Sequencer::Merge && matches!(operation, Operation::Skip) {
                    return Err(GitError(
                        "A merge cannot be skipped; abort it instead".into(),
                    ));
                }
                add(&[command, flag]);
            }
            Operation::BisectStart { good, bad } => {
                let good = self.resolve(good)?;
                let bad = self.resolve(bad)?;
                add(&["bisect", "start", &bad, &good]);
            }
            Operation::BisectMark { good } => {
                if status.operation != Some(Sequencer::Bisect) {
                    return Err(GitError("No bisect investigation is active".into()));
                }
                add(&["bisect", if *good { "good" } else { "bad" }]);
            }
            Operation::BisectSkip => {
                if status.operation != Some(Sequencer::Bisect) {
                    return Err(GitError("No bisect investigation is active".into()));
                }
                add(&["bisect", "skip"]);
            }
            Operation::BisectReset => {
                if status.operation != Some(Sequencer::Bisect) {
                    return Err(GitError("No bisect investigation is active".into()));
                }
                add(&["bisect", "reset"]);
            }
        }
        Ok(args)
    }

    fn resolve_stash(&self, id: &str) -> GitResult<Stash> {
        self.stashes()?
            .into_iter()
            .find(|stash| stash.id == id || stash.selector == id)
            .ok_or_else(|| GitError("Stash changed or no longer exists".into()))
    }
    fn validate_ref(&self, name: &str) -> GitResult<()> {
        if name.starts_with("[non-UTF8 ref: ") {
            return Err(GitError("Named non-UTF8 reference writes are not yet supported; inspect the exact saved SHA instead. No similarly named Unicode reference was changed.".into()));
        }
        if name.starts_with('-') || name.contains('\0') {
            return Err(GitError("Invalid branch or tag name".into()));
        }
        self.read(&["check-ref-format", "--branch", name])
            .map(|_| ())
    }
    fn validate_remote(&self, name: &str) -> GitResult<()> {
        if name.starts_with('-') || !self.remotes()?.iter().any(|remote| remote.name == name) {
            return Err(GitError("Select an existing named remote".into()));
        }
        Ok(())
    }
    fn bytes<I, S>(&self, args: I, stdin: Option<&[u8]>) -> GitResult<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let result = self.run(args, stdin)?;
        if result.success {
            Ok(result.stdout)
        } else {
            Err(result.error("Git operation"))
        }
    }

    /// Ordinary installed-Git semantics and the retained producer/window. No
    /// original-object flag, second runner, discovery, or renewed deadline.
    fn bytes_with_capture_limit_v2<I, S>(
        &self,
        args: I,
        stdin: Option<&[u8]>,
        cap: usize,
    ) -> GitResult<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        if cap == 0 || cap > MAX_OUTPUT {
            return Err(GitError("Invalid mutable read capture limit".into()));
        }
        let result = run_git_bounded_policy_v1(
            &self.root,
            args,
            stdin,
            false,
            &self.cancellation,
            GitReadPolicyV1 {
                timeout: COMMAND_TIMEOUT,
                output_limit: cap,
                window: self.navigation_read_window.as_ref(),
                runner: Some(self.actual_read_runner_v1()),
            },
        )?;
        if result.success {
            Ok(result.stdout)
        } else {
            Err(result.error("Git operation"))
        }
    }

    fn original_bytes<I, S>(&self, args: I, stdin: Option<&[u8]>) -> GitResult<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = std::iter::once(OsString::from("--no-replace-objects"))
            .chain(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        self.bytes(args, stdin)
    }
    fn original_text(&self, args: &[&str]) -> GitResult<String> {
        Ok(text(trim_line(&self.original_bytes(args.iter(), None)?)))
    }

    fn run<I, S>(&self, args: I, stdin: Option<&[u8]>) -> GitResult<GitOutput>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        run_git_with_window_v1(
            &self.root,
            args,
            stdin,
            false,
            &self.cancellation,
            self.navigation_read_window.as_ref(),
            Some(self.actual_read_runner_v1()),
        )
    }

    fn read(&self, args: &[&str]) -> GitResult<Vec<u8>> {
        let result = self.run(args, None)?;
        if result.success {
            Ok(result.stdout)
        } else {
            Err(result.error("Git"))
        }
    }

    fn read_text(&self, args: &[&str]) -> GitResult<String> {
        Ok(text(trim_line(&self.read(args)?)))
    }

    // Revision walking suppresses missing parents in shallow clones and may simplify file history.
    // Immutable commit object headers retain actual parent order and unloaded identities.
    fn canonical_parents(&self, mut commits: Vec<Commit>) -> GitResult<Vec<Commit>> {
        for chunk in commits.chunks_mut(1024) {
            let ids = chunk
                .iter()
                .map(|commit| commit.id.clone())
                .collect::<Vec<_>>();
            let objects = self.batch_objects(&ids, "commit")?;
            for (commit, object) in chunk.iter_mut().zip(objects) {
                commit.parents = original_parent_ids_v9(&object)?;
            }
        }
        Ok(commits)
    }
}

// SAME original-header parent scanner, also used for a just-created bounded
// commit object. This pure fact parser never mints a Source or permission.
fn original_parent_ids_v9(object: &[u8]) -> GitResult<Vec<String>> {
    let parents = object
        .split(|byte| *byte == b'\n')
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.strip_prefix(b"parent ").map(text))
        .collect::<Vec<_>>();
    for parent in &parents {
        validate_object_id(parent)?;
    }
    Ok(parents)
}
fn parse_tree(bytes: &[u8]) -> GitResult<Vec<TreeEntry>> {
    bytes
        .split(|b| *b == 0)
        .filter(|row| !row.is_empty())
        .map(|row| {
            let split = row
                .iter()
                .position(|b| *b == b'\t')
                .ok_or_else(|| GitError("Invalid tree record".into()))?;
            let fields = row[..split].split(|b| *b == b' ').collect::<Vec<_>>();
            if fields.len() != 3 {
                return Err(GitError("Invalid tree fields".into()));
            }
            Ok(TreeEntry {
                mode: text(fields[0]),
                kind: text(fields[1]),
                id: text(fields[2]),
                path: RawPath(row[split + 1..].to_vec()),
            })
        })
        .collect()
}

type DiffPaths = Vec<(Option<Vec<u8>>, Option<Vec<u8>>)>;
fn parse_diff_paths(bytes: &[u8]) -> GitResult<DiffPaths> {
    let mut records = bytes.split(|byte| *byte == 0);
    let mut paths = Vec::new();
    while let Some(status) = records.next() {
        if status.is_empty() {
            continue;
        }
        let path = records
            .next()
            .ok_or_else(|| GitError("Missing diff path".into()))?
            .to_vec();
        let pair = match status[0] {
            b'A' => (None, Some(path)),
            b'D' => (Some(path), None),
            b'R' | b'C' => {
                let target = records
                    .next()
                    .ok_or_else(|| GitError("Missing rename target".into()))?
                    .to_vec();
                (Some(path), Some(target))
            }
            _ => (Some(path.clone()), Some(path)),
        };
        paths.push(pair);
    }
    Ok(paths)
}

// SAME installed-Git worktree grammar shared by the ordinary reader and W9's
// exact paid post-write observation. No second parser or repository factory.
fn parse_worktrees_v9(bytes: &[u8]) -> Vec<Worktree> {
    let mut result = Vec::new();
    for field in bytes.split(|byte| *byte == 0) {
        if let Some(path) = field.strip_prefix(b"worktree ") {
            result.push(Worktree {
                path: PathBuf::from(OsString::from_vec(path.to_vec())),
                head: None,
                branch: None,
                bare: false,
                locked: false,
                prunable: false,
            });
        } else if let Some(worktree) = result.last_mut() {
            if let Some(head) = field.strip_prefix(b"HEAD ") {
                worktree.head = Some(text(head));
            } else if let Some(branch) = field.strip_prefix(b"branch ") {
                worktree.branch = Some(text(branch));
            } else if field == b"bare" {
                worktree.bare = true;
            } else if field.starts_with(b"locked") {
                worktree.locked = true;
            } else if field.starts_with(b"prunable") {
                worktree.prunable = true;
            }
        }
    }
    result
}

fn parse_status(bytes: &[u8]) -> GitResult<RepoStatus> {
    let mut status = RepoStatus::default();
    let mut records = bytes.split(|b| *b == 0);
    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        if let Some(value) = record.strip_prefix(b"# branch.oid ") {
            status.head = (value != b"(initial)").then(|| text(value));
        } else if let Some(value) = record.strip_prefix(b"# branch.head ") {
            status.detached = value == b"(detached)";
            status.branch = (!status.detached).then(|| reference_label(value));
            status.raw_branch = (!status.detached).then(|| value.to_vec());
        } else if let Some(value) = record.strip_prefix(b"# branch.upstream ") {
            status.upstream = Some(reference_label(value));
            status.raw_upstream = Some(value.to_vec());
        } else if let Some(value) = record.strip_prefix(b"# branch.ab ") {
            let parts = text(value);
            let mut fields = parts.split(' ');
            status.ahead = fields
                .next()
                .unwrap_or("+0")
                .trim_start_matches('+')
                .parse()
                .map_err(|_| GitError("Invalid ahead count".into()))?;
            status.behind = fields
                .next()
                .unwrap_or("-0")
                .trim_start_matches('-')
                .parse()
                .map_err(|_| GitError("Invalid behind count".into()))?;
        } else if record.starts_with(b"# ") {
            continue;
        } else if record.starts_with(b"? ") || record.starts_with(b"! ") {
            status.entries.push(StatusEntry {
                path: RawPath(record[2..].to_vec()),
                original_path: None,
                index: record[0] as char,
                worktree: record[0] as char,
                conflict: false,
                submodule: false,
            });
        } else {
            let fields_count = match record[0] {
                b'1' => 9,
                b'2' => 10,
                b'u' => 11,
                _ => return Err(GitError("Unknown Git status record".into())),
            };
            let fields = record
                .splitn(fields_count, |b| *b == b' ')
                .collect::<Vec<_>>();
            if fields.len() != fields_count || fields[1].len() != 2 {
                return Err(GitError("Malformed Git status record".into()));
            }
            let original_path = if record[0] == b'2' {
                Some(RawPath(
                    records
                        .next()
                        .ok_or_else(|| GitError("Missing rename source path".into()))?
                        .to_vec(),
                ))
            } else {
                None
            };
            status.entries.push(StatusEntry {
                path: RawPath(fields[fields_count - 1].to_vec()),
                original_path,
                index: fields[1][0] as char,
                worktree: fields[1][1] as char,
                conflict: record[0] == b'u',
                submodule: fields[2].starts_with(b"S"),
            });
        }
    }
    Ok(status)
}

fn parse_commits(bytes: &[u8]) -> GitResult<Vec<Commit>> {
    let mut fields = bytes.split(|b| *b == 0);
    let mut commits = Vec::new();
    while let Some(id) = fields.next() {
        let id = id.strip_prefix(b"\n").unwrap_or(id);
        if id.is_empty() {
            continue;
        }
        let mut values = Vec::with_capacity(9);
        for _ in 0..9 {
            values.push(
                fields
                    .next()
                    .ok_or_else(|| GitError("Truncated commit record".into()))?,
            );
        }
        commits.push(Commit {
            id: text(id),
            parents: text(values[0])
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            author: GitActor {
                name: display_bytes(values[1]),
                email: display_bytes(values[2]),
                timestamp: text(values[3])
                    .parse()
                    .map_err(|_| GitError("Invalid author timestamp".into()))?,
            },
            committer: GitActor {
                name: display_bytes(values[4]),
                email: display_bytes(values[5]),
                timestamp: text(values[6])
                    .parse()
                    .map_err(|_| GitError("Invalid committer timestamp".into()))?,
            },
            subject: display_bytes(values[7]),
            refs: split_decorations(values[8]).map(reference_label).collect(),
            raw_refs: split_decorations(values[8]).map(<[u8]>::to_vec).collect(),
        });
    }
    Ok(commits)
}

fn parse_blame(bytes: &[u8]) -> GitResult<Vec<BlameLine>> {
    let mut result = Vec::new();
    let mut current: Option<BlameLine> = None;
    for line in bytes.split(|b| *b == b'\n') {
        if let Some(content) = line.strip_prefix(b"\t") {
            let mut record = current
                .take()
                .ok_or_else(|| GitError("Missing blame header".into()))?;
            record.text = content.to_vec();
            result.push(record);
        } else if let Some(record) = &mut current {
            if let Some(author) = line.strip_prefix(b"author ") {
                record.author = display_bytes(author);
            } else if let Some(email) = line.strip_prefix(b"author-mail ") {
                record.email = display_bytes(email)
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_owned();
            } else if let Some(path) = line.strip_prefix(b"filename ") {
                record.path = RawPath(crate::diff::decode_quoted_path(path).map_err(GitError)?);
            }
        } else if !line.is_empty() {
            let fields = line.split(|b| *b == b' ').collect::<Vec<_>>();
            if fields.len() < 3 {
                return Err(GitError("Invalid blame header".into()));
            }
            validate_object_id(&text(fields[0]))?;
            current = Some(BlameLine {
                commit: text(fields[0]),
                original_line: text(fields[1])
                    .parse()
                    .map_err(|_| GitError("Invalid blame line".into()))?,
                final_line: text(fields[2])
                    .parse()
                    .map_err(|_| GitError("Invalid blame line".into()))?,
                author: String::new(),
                email: String::new(),
                path: RawPath(Vec::new()),
                text: Vec::new(),
            });
        }
    }
    Ok(result)
}

fn parse_three_fields(bytes: &[u8]) -> GitResult<Vec<(String, String, String)>> {
    let mut fields = bytes.split(|b| *b == 0);
    let mut rows = Vec::new();
    while let Some(first) = fields.next() {
        let first = first.strip_prefix(b"\n").unwrap_or(first);
        if first.is_empty() {
            continue;
        }
        let second = fields
            .next()
            .ok_or_else(|| GitError("Truncated record".into()))?;
        let third = fields
            .next()
            .ok_or_else(|| GitError("Truncated record".into()))?;
        rows.push((text(first), text(second), display_bytes(third)));
    }
    Ok(rows)
}

fn validate_path(path: &RawPath) -> GitResult<()> {
    let path_value = Path::new(path.as_os_str());
    if path.0.is_empty()
        || path.0.contains(&0)
        || path_value.is_absolute()
        || path_value
            .components()
            .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(GitError(
            "Path must name a file inside the repository".into(),
        ));
    }
    Ok(())
}
fn validate_object_id(id: &str) -> GitResult<()> {
    if !matches!(id.len(), 40 | 64) || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(GitError("Invalid immutable object identity".into()));
    }
    Ok(())
}
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn reference_label(bytes: &[u8]) -> String {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .unwrap_or_else(|_| format!("[non-UTF8 ref: {}]", display_path_bytes(bytes)))
}
fn reject_display_reference(value: &str) -> GitResult<()> {
    if value.starts_with("[non-UTF8 ref: ") {
        Err(GitError("Named non-UTF8 reference operations are not yet supported; inspect the exact saved SHA instead. No similarly named Unicode reference was changed.".into()))
    } else {
        Ok(())
    }
}
fn replacement_prefix(value: &OsStr) -> Vec<u8> {
    let mut prefix = value.as_bytes().to_vec();
    while prefix.last() == Some(&b'/') {
        prefix.pop();
    }
    prefix.push(b'/');
    prefix
}
fn split_decorations(mut bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    std::iter::from_fn(move || {
        if bytes.is_empty() {
            return None;
        }
        if let Some(split) = bytes.windows(2).position(|pair| pair == b", ") {
            let value = &bytes[..split];
            bytes = &bytes[split + 2..];
            Some(value)
        } else {
            let value = bytes;
            bytes = &[];
            Some(value)
        }
    })
}
fn trim_line(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}

struct GitOutput {
    success: bool,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
impl GitOutput {
    fn error(&self, context: &str) -> GitError {
        let mut messages = Vec::new();
        for (label, bytes) in [("stdout", &self.stdout), ("stderr", &self.stderr)] {
            let safe = git_diagnostic(bytes);
            if !safe.is_empty() {
                messages.push(format!("{label}: {safe}"));
            }
        }
        if messages.is_empty() {
            messages.push("Git exited unsuccessfully without a diagnostic".into());
        }
        GitError(format!("{context}:\n{}", messages.join("\n")))
    }
}

/// Bounded terminal-safe operation diagnostics, with transport URLs/known tokens redacted.
/// Saved user blobs/diffs must not pass through this display-only formatter.
pub fn display_git_diagnostic(bytes: &[u8]) -> String {
    git_diagnostic(bytes)
}

fn git_diagnostic(bytes: &[u8]) -> String {
    // Remote credentials and token-bearing URLs are not useful conflict diagnostics. Keep local
    // paths and Git's multiline resolution guidance, but redact entire transport URLs in both
    // streams rather than attempting to recognize every authentication query parameter.
    static URLS: OnceLock<regex::Regex> = OnceLock::new();
    let urls = URLS.get_or_init(|| {
        regex::Regex::new(r#"(?i)\b(?:https?|ssh|git|file|ext)://[^\s<>\"']+"#)
            .expect("Constant diagnostic URL regex")
    });
    let safe = crate::providers::runner::sanitize_terminal(&String::from_utf8_lossy(bytes));
    let safe = crate::providers::runner::redact(&safe, &[]);
    let safe = urls.replace_all(&safe, "[REMOTE URL REDACTED]");
    let mut result = safe.trim().chars().take(8192).collect::<String>();
    if safe.trim().chars().count() > 8192 {
        result.push_str("\n[Git diagnostic truncated]");
    }
    result
}
/// Raw `remote.<name>.url` config entries (NUL-separated `key\nvalue`),
/// read under the standard read-only policy. None when absent or failing.
pub fn remote_url_config_v1(directory: &Path) -> Option<Vec<u8>> {
    let output = run_git_with_kind(
        directory,
        ["config", "--null", "--get-regexp", r"^remote\..*\.url$"],
        None,
        false,
        &CancellationToken::default(),
    )
    .ok()?;
    output.success.then_some(output.stdout)
}
fn run_git_with_kind<I, S>(
    directory: &Path,
    args: I,
    stdin: Option<&[u8]>,
    write: bool,
    cancellation: &CancellationToken,
) -> GitResult<GitOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_git_bounded_with_kind(
        directory,
        args,
        stdin,
        write,
        cancellation,
        COMMAND_TIMEOUT,
        MAX_OUTPUT,
    )
}

fn run_git_bounded_with_kind<I, S>(
    directory: &Path,
    args: I,
    stdin: Option<&[u8]>,
    write: bool,
    cancellation: &CancellationToken,
    timeout: Duration,
    output_limit: usize,
) -> GitResult<GitOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_git_bounded_policy_v1(
        directory,
        args,
        stdin,
        write,
        cancellation,
        GitReadPolicyV1 {
            timeout,
            output_limit,
            window: None,
            runner: None,
        },
    )
}

fn run_git_bounded_policy_v1<I, S>(
    directory: &Path,
    args: I,
    stdin: Option<&[u8]>,
    write: bool,
    cancellation: &CancellationToken,
    policy: GitReadPolicyV1<'_>,
) -> GitResult<GitOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_git_bounded_policy_deadline_v2(directory, args, stdin, write, cancellation, policy, None)
}
fn run_git_bounded_policy_deadline_v2<I, S>(
    directory: &Path,
    args: I,
    stdin: Option<&[u8]>,
    write: bool,
    cancellation: &CancellationToken,
    policy: GitReadPolicyV1<'_>,
    absolute_deadline: Option<std::time::Instant>,
) -> GitResult<GitOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let spec = git_command_spec_v9(directory, args, stdin, write, &policy, absolute_deadline);
    let output = if let Some(window) = policy.window {
        window.run(spec, cancellation)
    } else {
        policy
            .runner
            .unwrap_or_else(|| RUNNER.get_or_init(CommandRunner::default))
            .run(spec, cancellation)
    };
    git_command_output_v9(output)
}

// Private builder reused by the closed W9 post-read boundary. No general
// provider/spec/run getter is exported; ordinary argv/env/limits stay exact.
fn git_command_spec_v9<I, S>(
    directory: &Path,
    args: I,
    stdin: Option<&[u8]>,
    write: bool,
    policy: &GitReadPolicyV1<'_>,
    absolute_deadline: Option<std::time::Instant>,
) -> CommandSpec
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_owned())
        .collect::<Vec<_>>();
    let continue_existing_message = write && args.iter().any(|arg| arg == "--continue");
    let original_objects = args.iter().any(|arg| arg == "--no-replace-objects");
    // ls-remote is only used by an explicitly targeted force-lease preview. Ordinary local
    // browsing must not allow promisor-object reads to invoke implicit transport/network fetch.
    let explicit_transport = write || args.first().is_some_and(|arg| arg == "ls-remote");
    let mut spec = CommandSpec::new("git")
        .args(["--no-pager", "--literal-pathspecs", "-c", "color.ui=false"])
        // GIT_OPTIONAL_LOCKS alone does not prevent porcelain diff from
        // refreshing stat-cache entries after a cached apply. Read-only
        // browsing must preserve raw index bytes as well as staged contents.
        // Keep explicit writes on their unchanged policy/argv.
        .args(if write {
            &[][..]
        } else {
            &["-c", "diff.autoRefreshIndex=false"][..]
        })
        // Original-object reads deliberately neutralize graft files below.
        // Suppress only the resulting advice, not stderr/errors or capture
        // limits: even a valid full-SHA read must fit its small raw envelope.
        .args(if original_objects {
            &["-c", "advice.graftFileDeprecated=false"][..]
        } else {
            &[][..]
        })
        .args(args)
        .cwd(directory)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_PAGER", "cat")
        // Continuing a confirmed operation accepts its existing Git message; new messages use
        // explicit commit stdin. Other unexpected editor requests fail with a visible diagnostic.
        .env(
            "GIT_EDITOR",
            if continue_existing_message {
                "true"
            } else {
                "false"
            },
        )
        .env("GIT_SEQUENCE_EDITOR", "false")
        .input(stdin.unwrap_or_default())
        .timeout(COMMAND_TIMEOUT.min(policy.timeout));
    spec.output_limit = MAX_OUTPUT.min(policy.output_limit);
    spec.absolute_deadline_v1 = absolute_deadline;
    #[cfg(test)]
    local_commit_actor_origin_v1::trace_spec_for_tests(&mut spec, directory);
    if original_objects {
        // A graft file can rewrite parent traversal independently of replacement refs.
        spec = spec.env("GIT_GRAFT_FILE", "/dev/null");
    }
    if !explicit_transport {
        // Older Git versions do not recognize GIT_NO_LAZY_FETCH. The empty protocol allowlist
        // denies every transport even when repository configuration permits a custom helper.
        spec = spec.env("GIT_ALLOW_PROTOCOL", "");
    }
    if write {
        spec = spec.write(format!("git:{}", directory.display()));
    }
    spec
}
fn git_command_output_v9(
    output: Result<
        crate::providers::runner::CommandOutput,
        crate::providers::runner::CommandFailure,
    >,
) -> GitResult<GitOutput> {
    match output {
        Ok(output) => Ok(GitOutput {
            success: true,
            exit_code: Some(output.exit_code),
            stdout: output.stdout,
            stderr: output.stderr,
        }),
        Err(error) if error.kind == FailureKind::Exit => Ok(GitOutput {
            success: false,
            exit_code: error.exit_code,
            stdout: error.stdout,
            stderr: error.stderr,
        }),
        Err(error) => Err(GitError(error.to_string())),
    }
}

#[cfg(test)]
#[path = "git/readonly_index_v1_tests.rs"]
mod readonly_index_v1_tests;

/// True when `git bisect` output (or `git bisect log`) reports the culprit.
/// Git 2.55 quotes the term ("first 'bad' commit"); older Git does not.
pub fn bisect_reports_culprit(text: &str) -> bool {
    text.contains("first bad commit") || text.contains("first 'bad' commit")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_explains_stale_core_worktree_pointing_elsewhere() {
        let checkout = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            assert!(std::process::Command::new("git")
                .args(args)
                .current_dir(checkout.path())
                .status()
                .unwrap()
                .success());
        };
        git(&["init", "-q"]);
        git(&[
            "config",
            "core.worktree",
            elsewhere.path().to_str().unwrap(),
        ]);
        let error = GitRepository::discover_with_cancellation(
            checkout.path(),
            CancellationToken::default(),
        )
        .unwrap_err()
        .0;
        assert!(
            error
                .lines()
                .next()
                .unwrap()
                .contains("core.worktree points to"),
            "{error}"
        );
        assert!(
            error
                .lines()
                .any(|line| line.trim() == "git config --unset core.worktree"),
            "{error}"
        );
        git(&["config", "--unset", "core.worktree"]);
        assert!(GitRepository::discover_with_cancellation(
            checkout.path(),
            CancellationToken::default()
        )
        .unwrap()
        .is_some());
    }
    #[test]
    fn immutable_overrides_raw_replacement_base_does_not_fall_back_or_alias_unicode() {
        let raw = OsStr::from_bytes(b"refs/custom\xff///");
        assert_eq!(replacement_prefix(raw), b"refs/custom\xff/");
        assert_ne!(
            replacement_prefix(raw),
            replacement_prefix(OsStr::new("refs/custom\u{fffd}"))
        );
        assert_ne!(
            reference_label(b"refs/heads/na\xff"),
            reference_label("refs/heads/na\u{fffd}".as_bytes())
        );
    }
    #[test]
    fn git_provider_raw_status_renames_conflicts_and_unborn() {
        let bytes = b"# branch.oid (initial)\0# branch.head main\0? tab\tline\n\xff\0";
        let status = parse_status(bytes).unwrap();
        assert_eq!(status.head, None);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.entries[0].path.0, b"tab\tline\n\xff");
        assert_eq!(status.entries[0].path.display(), "tab\\tline\\n\\xff");
    }
    #[test]
    fn git_provider_rejects_traversal_and_control_injection() {
        assert!(validate_path(&RawPath(b"../outside".to_vec())).is_err());
        assert!(validate_path(&RawPath(b"/absolute".to_vec())).is_err());
        assert!(validate_path(&RawPath(b"a\0b".to_vec())).is_err());
        assert_eq!(display_bytes(b"title\x1b[2J"), "title\\u{1b}[2J");
    }

    #[test]
    fn git_sequencers_failure_keeps_conflict_stdout_and_safe_bounded_multiline_stderr() {
        let output = GitOutput {
            success: false,
            exit_code: Some(1),
            stdout: b"\x1b[31mCONFLICT (content): Merge conflict in file.rs\x1b[0m\nFix conflicts and then commit the result.\n".to_vec(),
            stderr: b"remote: https://alice:password@example.invalid/private?token=secret\nAuthorization ghp_privateFixtureToken\n".to_vec(),
        };
        let error = output.error("Git operation").0;
        assert!(error.contains("stdout: CONFLICT (content)"));
        assert!(error.contains("\nFix conflicts and then commit"));
        assert!(error.contains("stderr: remote: [REMOTE URL REDACTED]"));
        for secret in [
            "password",
            "secret",
            "privateFixtureToken",
            "example.invalid",
            "\x1b",
        ] {
            assert!(!error.contains(secret));
        }
        let bounded = git_diagnostic(&vec![b'a'; 32768]);
        assert_eq!(
            bounded
                .chars()
                .take_while(|character| *character == 'a')
                .count(),
            8192
        );
        assert!(bounded.ends_with("[Git diagnostic truncated]"));
    }

    #[test]
    fn remote_git_preview_urls_preserve_host_repository_without_authentication_material() {
        for value in [
            "https://alice:secret@example.invalid/org/repo.git?token=private#password",
            "ssh://alice:secret@example.invalid/org/repo.git?signature=private",
            "alice:secret@example.invalid:org/repo.git?token=private",
        ] {
            let label = display_remote_url(value);
            assert!(label.contains("example.invalid") && label.contains("org/repo.git"));
            assert!(
                label.contains("[userinfo redacted]")
                    && label.contains("[query/fragment redacted]")
            );
            for secret in [
                "alice",
                "secret",
                "private",
                "password",
                "signature",
                "token=",
            ] {
                assert!(!label.contains(secret));
            }
        }
        assert_eq!(
            display_remote_url("https://example.invalid/org/repo.git"),
            "https://example.invalid/org/repo.git"
        );
        assert_eq!(
            display_remote_url("file:///tmp/local.git"),
            "file:///tmp/local.git"
        );
        assert_eq!(
            display_remote_url("/tmp/local?literal#name"),
            "/tmp/local?literal#name"
        );
        assert!(
            !display_remote_url("https://example.invalid/org/ghp_privateToken")
                .contains("privateToken")
        );
    }
}
