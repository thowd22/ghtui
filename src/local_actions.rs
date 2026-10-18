//! Native local-operation intents. Forms only gather values; writes require a fresh preview
//! followed by the same explicitly confirmed, state-checked Git operation.
use crate::{
    git::{
        Comparison, ComparisonResult, GitRepository, MutationPlan, Operation, PullStrategy,
        RawPath, Reference, ReflogEntry, Remote, RepoStatus, ResetMode, Stash, Worktree,
    },
    providers::runner::{sanitize_terminal, CancellationToken},
    repository_view::{diff_rows, DiffAnchor, DiffLayout},
    workspace::{DisplayRow, RepositoryMode, RowAction, Workspace},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalCommand {
    StageFile,
    UnstageFile,
    StageSelection,
    UnstageSelection,
    Commit,
    Amend,
    CreateBranch,
    RenameBranch,
    SwitchBranch,
    DeleteBranch,
    CreateTag,
    DeleteTag,
    SetUpstream,
    Fetch,
    Pull,
    Push,
    ForcePushLease,
    CreatePullRequest,
    StashSave,
    StashApply,
    StashPop,
    StashDrop,
    AddWorktree,
    RemoveWorktree,
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Continue,
    Skip,
    Abort,
    ResolveConflict,
    RecoverBranch,
    Reset,
    DiscardFile,
    BisectStart,
    BisectGood,
    BisectBad,
    BisectSkip,
    BisectReset,
}
impl LocalCommand {
    pub const ALL: [(Self, &'static str); 40] = [
        (Self::StageFile, "Stage selected file"),
        (Self::UnstageFile, "Unstage selected file"),
        (Self::StageSelection, "Stage selected lines"),
        (Self::UnstageSelection, "Unstage selected lines"),
        (Self::Commit, "Commit staged changes"),
        (Self::Amend, "Amend latest commit"),
        (Self::CreateBranch, "Create branch"),
        (Self::RenameBranch, "Rename branch"),
        (Self::SwitchBranch, "Switch branch"),
        (Self::DeleteBranch, "Delete branch"),
        (Self::CreateTag, "Create tag"),
        (Self::DeleteTag, "Delete tag"),
        (Self::SetUpstream, "Set upstream"),
        (Self::Fetch, "Fetch remote"),
        (Self::Pull, "Pull remote branch"),
        (Self::Push, "Push branch"),
        (Self::ForcePushLease, "Force push with exact lease"),
        (Self::CreatePullRequest, "Create pull request"),
        (Self::StashSave, "Save stash"),
        (Self::StashApply, "Apply stash"),
        (Self::StashPop, "Apply and remove stash"),
        (Self::StashDrop, "Delete stash"),
        (Self::AddWorktree, "Add linked worktree"),
        (Self::RemoveWorktree, "Remove linked worktree"),
        (Self::Merge, "Merge saved history"),
        (Self::Rebase, "Rebase onto saved history"),
        (Self::CherryPick, "Cherry-pick commit"),
        (Self::Revert, "Revert commit"),
        (Self::Continue, "Continue operation"),
        (Self::Skip, "Skip current commit"),
        (Self::Abort, "Abort operation"),
        (Self::ResolveConflict, "Choose conflict version"),
        (Self::RecoverBranch, "Create rescue branch"),
        (Self::Reset, "Reset current branch"),
        (Self::DiscardFile, "Discard selected file changes"),
        (Self::BisectStart, "Start bug search"),
        (Self::BisectGood, "Mark version good"),
        (Self::BisectBad, "Mark version bad"),
        (Self::BisectSkip, "Skip untestable version"),
        (Self::BisectReset, "End bug search"),
    ];
    pub fn label(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(command, _)| *command == self)
            .unwrap()
            .1
    }
    pub fn key(self) -> String {
        format!("{self:?}").to_ascii_lowercase()
    }
    pub fn disabled_reason(self, context: &LocalContext) -> Option<String> {
        crate::local_availability::LocalAvailability::from_context(context).disabled_reason(self)
    }
}

/// Direct staging/commit shortcuts. They open the same Local intent as the
/// menu, but staging arrives with its read-only preview already prepared, so
/// one confirmation (Ctrl-S or [Confirm action]) remains the only write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickLocal {
    Stage,
    Unstage,
    Commit,
}
impl QuickLocal {
    pub const CONTROLS: [(Self, &'static str, char); 3] = [
        (Self::Stage, "[+ Stage]", '+'),
        (Self::Unstage, "[- Unstage]", '-'),
        (Self::Commit, "[K Commit]", 'K'),
    ];
    /// Chooses the exact command for the current row or explicit line/hunk
    /// selection. Nothing is guessed from labels.
    pub fn command(
        self,
        row: Option<&RowAction>,
        line_selection: bool,
    ) -> Result<LocalCommand, &'static str> {
        let stage = match self {
            Self::Commit => return Ok(LocalCommand::Commit),
            Self::Stage => true,
            Self::Unstage => false,
        };
        if line_selection {
            return Ok(if stage {
                LocalCommand::StageSelection
            } else {
                LocalCommand::UnstageSelection
            });
        }
        match row {
            Some(RowAction::WorkingFile(_) | RowAction::Conflict(_) | RowAction::File(_)) => {
                Ok(if stage {
                    LocalCommand::StageFile
                } else {
                    LocalCommand::UnstageFile
                })
            }
            _ => Err("Select a changed file, diff line or hunk first • + Stage • - Unstage"),
        }
    }
    /// Staging previews have no fields, so they are prepared immediately.
    pub fn previews_immediately(command: LocalCommand) -> bool {
        matches!(
            command,
            LocalCommand::StageFile
                | LocalCommand::UnstageFile
                | LocalCommand::StageSelection
                | LocalCommand::UnstageSelection
        )
    }
}

/// Prepares the read-only plan for a fieldless Local form so its preview is
/// visible at once. Confirmation still re-checks state before writing.
pub fn preview_form(
    form: &mut crate::forms::Form,
    cancellation: CancellationToken,
) -> Result<(), String> {
    let values = form.values();
    let crate::forms::FormPurpose::Local(intent) = &mut form.purpose else {
        return Err("Only Local intents have a staging preview".into());
    };
    intent.plan = Some(LocalPlan::prepare_selected(
        intent.command,
        &intent.context,
        &values,
        &intent.selections,
        cancellation,
    )?);
    form.preview = true;
    form.preview_scroll = 0;
    form.error.clear();
    Ok(())
}

/// Names what a completed staging operation touched, for the status line.
pub fn completion_target(command: LocalCommand, context: &LocalContext) -> String {
    let path = context
        .selected_path
        .as_ref()
        .map(|path| path.display().to_string());
    match (command, path) {
        (
            LocalCommand::StageFile | LocalCommand::UnstageFile | LocalCommand::DiscardFile,
            Some(path),
        ) => format!(" • {path}"),
        (LocalCommand::StageSelection | LocalCommand::UnstageSelection, Some(path)) => {
            let lines = context.selected_lines.len();
            format!(
                " • {lines} line{} in {path}",
                if lines == 1 { "" } else { "s" }
            )
        }
        _ => String::new(),
    }
}

/// Staging keeps the user in the diff they were working through.
pub fn refreshed_mode(command: LocalCommand, context: &LocalContext) -> RepositoryMode {
    if !QuickLocal::previews_immediately(command) {
        return RepositoryMode::Overview;
    }
    match context.comparison.as_ref().map(|result| &result.comparison) {
        Some(Comparison::Working) => RepositoryMode::Working,
        Some(Comparison::Staged) => RepositoryMode::Staged,
        Some(Comparison::Uncommitted) => RepositoryMode::Uncommitted,
        _ => RepositoryMode::Overview,
    }
}

#[derive(Clone, Debug)]
pub struct LocalContext {
    pub directory: PathBuf,
    pub revision: String,
    pub status: RepoStatus,
    pub selected_path: Option<RawPath>,
    pub comparison: Option<ComparisonResult>,
    pub selected_anchor: Option<DiffAnchor>,
    /// An explicit multi-line selection from this same canonical rendered file.
    pub selected_lines: Vec<(usize, usize)>,
}
impl LocalContext {
    pub fn from_workspace(
        workspace: &Workspace,
        selected_path: Option<RawPath>,
        selected_anchor: Option<DiffAnchor>,
    ) -> Self {
        let selected_path = selected_path.or_else(|| {
            selected_anchor
                .as_ref()
                .and_then(|anchor| anchor.new_path.clone().or_else(|| anchor.old_path.clone()))
        });
        let selected_lines = selected_anchor
            .as_ref()
            .map(|anchor| vec![(anchor.hunk_index, anchor.line_index)])
            .unwrap_or_default();
        Self {
            directory: workspace.directory.clone(),
            revision: workspace.revision.clone(),
            status: workspace.status.clone(),
            selected_path,
            comparison: workspace.comparison.clone(),
            selected_anchor,
            selected_lines,
        }
    }
    pub fn from_row(workspace: &Workspace, row: usize) -> Self {
        let action = workspace
            .display_rows_v1()
            .get(row)
            .and_then(|row| row.action.as_ref());
        let path = match action {
            Some(
                RowAction::WorkingFile(path) | RowAction::Conflict(path) | RowAction::File(path),
            ) => Some(path.clone()),
            _ => None,
        };
        let anchor = match action {
            Some(RowAction::DiffLine(anchor)) => Some(anchor.clone()),
            _ => None,
        };
        Self::from_workspace(workspace, path, anchor)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalField {
    pub key: &'static str,
    pub label: &'static str,
    pub value: String,
    pub multiline: bool,
    pub choices: Vec<&'static str>,
}
fn field(key: &'static str, label: &'static str, value: &str) -> LocalField {
    LocalField {
        key,
        label,
        value: value.into(),
        multiline: false,
        choices: vec![],
    }
}
fn choice(
    key: &'static str,
    label: &'static str,
    value: &str,
    choices: &[&'static str],
) -> LocalField {
    LocalField {
        choices: choices.into(),
        ..field(key, label, value)
    }
}
pub fn fields(command: LocalCommand, context: &LocalContext) -> Vec<LocalField> {
    use LocalCommand::*;
    let branch = context.status.branch.as_deref().unwrap_or("");
    let revision = context.revision.as_str();
    match command {
        Commit | Amend => vec![LocalField {
            multiline: true,
            ..field("message", "Full commit message", "")
        }],
        CreateBranch | CreateTag | RecoverBranch => vec![
            field("name", "New name", ""),
            field("revision", "Saved version", revision),
        ],
        RenameBranch => vec![
            field("old", "Existing branch", branch),
            field("new", "New branch name", ""),
        ],
        SwitchBranch | DeleteTag => vec![field("name", "Existing name", "")],
        DeleteBranch => vec![
            field("name", "Existing branch", ""),
            choice(
                "force",
                "Delete unmerged history?",
                "false",
                &["false", "true"],
            ),
        ],
        SetUpstream => vec![
            field("branch", "Local branch", branch),
            field("upstream", "Tracking branch (remote/name)", ""),
        ],
        Fetch => vec![field("remote", "Configured remote", "origin")],
        Pull => vec![
            field("remote", "Configured remote", "origin"),
            field("branch", "Remote branch", branch),
            choice(
                "strategy",
                "Pull strategy",
                "ff-only",
                &["ff-only", "merge", "rebase"],
            ),
        ],
        Push => vec![
            field("remote", "Configured remote", "origin"),
            field("branch", "Local branch", branch),
            choice(
                "set_upstream",
                "Set tracking branch?",
                // A branch without upstream is usually being published for the first time.
                if context.status.upstream.is_none() && !branch.is_empty() {
                    "true"
                } else {
                    "false"
                },
                &["false", "true"],
            ),
        ],
        ForcePushLease => vec![
            field("remote", "Configured remote", "origin"),
            field("branch", "Local branch", branch),
            field("expected_remote", "Exact expected remote commit SHA", ""),
        ],
        CreatePullRequest => vec![
            field("title", "Pull request title", ""),
            LocalField {
                multiline: true,
                ..field("body", "Description (optional)", "")
            },
            field("base", "Base branch (empty = repository default)", ""),
            choice("draft", "Open as draft?", "false", &["false", "true"]),
        ],
        StashSave => vec![
            field("message", "Stash description", ""),
            choice(
                "untracked",
                "Include untracked files?",
                "false",
                &["false", "true"],
            ),
        ],
        StashApply | StashPop | StashDrop => vec![field("id", "Exact stash commit SHA", "")],
        AddWorktree => vec![
            field("path", "New worktree directory", ""),
            field("branch", "Existing branch", ""),
        ],
        RemoveWorktree => vec![field("path", "Exact linked worktree directory", "")],
        Merge | Rebase | CherryPick | Revert => vec![field("revision", "Saved version", revision)],
        ResolveConflict => vec![choice(
            "stage",
            "Keep version (ours = current branch, theirs = incoming, base = common ancestor)",
            "ours",
            &["ours", "theirs", "base"],
        )],
        Reset => vec![
            field("revision", "Saved version", revision),
            choice("mode", "Reset effect", "soft", &["soft", "mixed", "hard"]),
        ],
        BisectStart => vec![
            field("good", "Known good saved version", ""),
            field("bad", "Known bad saved version", revision),
        ],
        _ => vec![],
    }
}

#[derive(Clone, Debug)]
pub struct LocalPlan {
    pub command: LocalCommand,
    pub context: LocalContext,
    pub mutation: MutationPlan,
    pub preview: Vec<String>,
}
fn preview_header_v2(command: LocalCommand, mutation: &MutationPlan) -> Vec<String> {
    vec![
        format!("{} — no changes until confirmation", command.label()),
        format!("Target: {}", redact_remote_credentials(&mutation.target)),
        format!("Git effect: {}", sanitize_terminal(&mutation.effect)),
    ]
}
fn append_selected_patch_preview_v2(preview: &mut Vec<String>, mutation: &MutationPlan) {
    if let Operation::ApplyPatch { patch, .. } = &mutation.operation {
        preview.extend(
            String::from_utf8_lossy(patch)
                .lines()
                .map(sanitize_terminal),
        );
    }
}
/// Closed in-memory continuity of an ACTUAL staged patch already read for
/// this LocalPlan. No constructor, Clone, serde, bytes or digest getter.
pub(crate) struct WorkflowReviewedPatchV9 {
    digest: [u8; 32],
    raw_len: usize,
    base: String,
    observed: crate::git::WorkspaceFingerprint,
}
impl std::fmt::Debug for WorkflowReviewedPatchV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowReviewedPatchV9(<actual staged canonical bytes; no permission>)")
    }
}
impl WorkflowReviewedPatchV9 {
    // The inline value/header is already charged in its containing Preview.
    pub(crate) fn retained_heap_bytes_v9(&self) -> usize {
        self.base.capacity()
    }
    pub(crate) fn binds_plan_v9(&self, plan: &LocalPlan, base: &str) -> bool {
        self.base == base
            && self.observed == plan.mutation.observed
            && matches!(
                &plan.mutation.operation,
                Operation::Commit { amend: false, .. }
            )
    }
    pub(crate) fn matches_post_v9(&self, actual: &ComparisonResult) -> bool {
        use sha2::{Digest, Sha256};
        matches!(&actual.comparison, Comparison::Snapshots { base, head }
            if base == &self.base && actual.base.as_deref() == Some(base.as_str())
                && actual.head.as_deref() == Some(head.as_str()))
            && actual.patch.len() == self.raw_len
            && <[u8; 32]>::from(Sha256::digest(&actual.patch)) == self.digest
    }
}
impl LocalPlan {
    /// SAME local planner/preview, accepting only an actual matching original
    /// workflow and a captured absent private child. No fallback construction
    /// from a destination pathname, saved mapping or generic Form fields.
    pub(crate) fn prepare_workflow_worktree_v9(
        repo: &GitRepository,
        purpose: &crate::git::workflow_edit_v9::WorkflowWorktreePurposeV9,
    ) -> Result<Self, String> {
        let safe = || "Original workflow/worktree target changed; no write or fallback".to_owned();
        purpose.validate(repo).map_err(|_| safe())?;
        let status = repo.status().map_err(|_| safe())?;
        purpose.validate_status(&status).map_err(|_| safe())?;
        let mutation = repo
            .plan_workflow_worktree_v9(purpose)
            .map_err(|_| safe())?;
        let target = purpose.target();
        let mut preview = preview_header_v2(LocalCommand::AddWorktree, &mutation);
        preview.extend([
            format!("New isolated worktree: {}", crate::git::display_bytes(target.path().as_os_str().as_encoded_bytes())),
            format!("NEW branch (must not already exist): {}", target.values().branch()),
            format!("Exact original workflow commit C: {}", target.opened().intent().definition().commit_sha),
            "Explicit Confirm runs only worktree add -b NEW -- ABSENT_PATH C.".into(),
            "Creates the directory, linked-worktree metadata and new shared local branch. No remote write, existing-file Save, stash, reset, force or automatic cleanup.".into(),
            "Git hooks/signing remain enabled. External filesystem/configuration races cannot be claimed atomically excluded; ambiguity requires Inspect, not retry/delete.".into(),
        ]);
        let context = LocalContext {
            directory: repo.root.clone(),
            revision: status.head.clone().ok_or_else(safe)?,
            status,
            selected_path: None,
            comparison: None,
            selected_anchor: None,
            selected_lines: Vec::new(),
        };
        purpose.validate(repo).map_err(|_| safe())?;
        Ok(Self {
            command: LocalCommand::AddWorktree,
            context,
            mutation,
            preview,
        })
    }
    /// W9 borrows the SAME actual repository and planner inside its one
    /// measured original worker; never rediscover through legacy defaults.
    pub(crate) fn prepare_workflow_edit_v9(
        repo: &GitRepository,
        purpose: &crate::git::workflow_edit_v9::WorkflowPlanPurposeV9,
    ) -> Result<Self, String> {
        Self::prepare_workflow_edit_with_review_v9(repo, purpose).map(|(plan, _)| plan)
    }
    pub(crate) fn prepare_workflow_edit_with_review_v9(
        repo: &GitRepository,
        purpose: &crate::git::workflow_edit_v9::WorkflowPlanPurposeV9,
    ) -> Result<(Self, Option<WorkflowReviewedPatchV9>), String> {
        use crate::git::workflow_edit_v9::WorkflowLocalStepV9;
        let safe = || {
            "Original workflow file/index/plan changed; no local write or private diagnostics"
                .to_owned()
        };
        purpose.validate(repo).map_err(|_| safe())?;
        let status = repo.status().map_err(|_| safe())?;
        purpose.validate_status(&status).map_err(|_| safe())?;
        let staged = if purpose.step() == WorkflowLocalStepV9::Commit {
            let staged = repo
                .compare_with_capture_limit_v1(&Comparison::Staged, 256 * 1024)
                .map_err(|_| safe())?;
            let path = purpose.path();
            if staged.diff.files.is_empty()
                || staged.diff.files.iter().any(|file| {
                    file.new_path.as_deref() != Some(path.0.as_slice())
                        || file
                            .old_path
                            .as_deref()
                            .is_some_and(|old| old != path.0.as_slice())
                })
            {
                return Err(safe());
            }
            Some(staged)
        } else {
            None
        };
        let mutation = repo.plan_workflow_edit_v9(purpose).map_err(|_| safe())?;
        if staged
            .as_ref()
            .is_some_and(|comparison| comparison.observed != Some(mutation.observed))
        {
            return Err(safe());
        }
        let reviewed = if let Some(staged) = &staged {
            use sha2::{Digest, Sha256};
            let base = staged
                .base
                .as_deref()
                .filter(|base| {
                    *base == purpose.original_base_v9()
                        && crate::git::original_actor_saved_sha(base)
                })
                .ok_or_else(safe)?;
            // Borrowed hash of the SAME actual canonical patch; not a new
            // child/body clone or a serialized input/source authorization.
            Some(WorkflowReviewedPatchV9 {
                digest: Sha256::digest(&staged.patch).into(),
                raw_len: staged.patch.len(),
                base: base.to_owned(),
                observed: mutation.observed,
            })
        } else {
            None
        };
        let body = if let Some(staged) = &staged {
            std::str::from_utf8(&staged.patch).map_err(|_| safe())?
        } else {
            purpose.saved_content()
        };
        let rows = body.split('\n').count();
        let bound = body
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add((rows + 8) * (std::mem::size_of::<String>() + 8)))
            .ok_or_else(safe)?;
        if rows > 65_500 || bound > 4 * 1024 * 1024 {
            return Err(safe());
        }
        let mut preview = preview_header_v2(purpose.command(), &mutation);
        preview.reserve(rows + 4);
        preview.push(format!(
            "Exact original workflow file: {}",
            purpose.path().display()
        ));
        preview.push(if staged.is_some() {
            "Complete actual staged canonical patch; this Commit does not include other paths:"
                .into()
        } else {
            "Stages the whole actual saved file below, not display-row coordinates or other paths:"
                .into()
        });
        preview.extend(body.split('\n').map(sanitize_terminal));
        let context = LocalContext {
            directory: repo.root.clone(),
            revision: status.head.clone().ok_or_else(safe)?,
            status,
            selected_path: Some(purpose.path()),
            comparison: None,
            selected_anchor: None,
            selected_lines: Vec::new(),
        };
        purpose.validate(repo).map_err(|_| safe())?;
        Ok((
            Self {
                command: purpose.command(),
                context,
                mutation,
                preview,
            },
            reviewed,
        ))
    }
    /// Native-only borrowed canonical selection. No Workspace/context body is
    /// cloned, and the existing installed-Git selected-patch planner is reused.
    pub(crate) fn prepare_native_selection_v2(
        repo: &GitRepository,
        comparison: &ComparisonResult,
        file_index: usize,
        selected: &[(usize, usize)],
        reverse: bool,
    ) -> Result<Self, String> {
        if selected.is_empty() || selected.len() > 10_000 {
            return Err("Select between1 and10000 original changed lines".into());
        }
        let command = if reverse {
            LocalCommand::UnstageSelection
        } else {
            LocalCommand::StageSelection
        };
        let file = comparison
            .diff
            .files
            .get(file_index)
            .ok_or("Selected original file unavailable")?;
        let (hunk, line) = selected[0];
        let original = file
            .hunks
            .get(hunk)
            .and_then(|hunk| hunk.lines.get(line))
            .ok_or("Selected original changed line unavailable")?;
        if !matches!(
            original.kind,
            crate::diff::LineKind::Added | crate::diff::LineKind::Removed
        ) {
            return Err("Only genuine changed lines can be previewed".into());
        }
        let anchor = DiffAnchor {
            base: comparison.base.clone(),
            head: comparison.head.clone(),
            old_path: file.old_path.clone().map(RawPath::new),
            new_path: file.new_path.clone().map(RawPath::new),
            old_line: original.old_line,
            new_line: original.new_line,
            side: if original.kind == crate::diff::LineKind::Added {
                crate::repository_view::DiffSide::New
            } else {
                crate::repository_view::DiffSide::Old
            },
            file_index,
            hunk_index: hunk,
            line_index: line,
        };
        let status = repo.status().map_err(|error| error.to_string())?;
        if let Some(reason) = crate::local_availability::LocalAvailability::for_native_selection_v2(
            &status,
            &anchor,
            &comparison.comparison,
        )
        .disabled_reason(command)
        {
            return Err(reason);
        }
        if status.entries.iter().any(|entry| {
            entry.conflict
                && (Some(&entry.path) == anchor.old_path.as_ref()
                    || Some(&entry.path) == anchor.new_path.as_ref())
        }) {
            return Err(
                "Selected file is conflicted; inspect the explicit conflict viewer instead".into(),
            );
        }
        let mutation = repo
            .plan_selected_patch(comparison, file_index, selected, reverse)
            .map_err(|error| error.to_string())?;
        let Operation::ApplyPatch { patch, .. } = &mutation.operation else {
            return Err("Selected preview did not produce the shared ApplyPatch plan".into());
        };
        if patch.len() > 2 * 1024 * 1024 {
            return Err("Selected patch exceeds the2MiB native confirmation envelope; no write was prepared".into());
        }
        let mut preview = preview_header_v2(command, &mutation);
        append_selected_patch_preview_v2(&mut preview, &mutation);
        let bytes = preview.iter().map(String::len).sum::<usize>();
        if bytes > 8 * 1024 * 1024 || preview.len() > 50_000 {
            return Err("Selected native preview exceeds bounded display material".into());
        }
        if repo.fingerprint().map_err(|error| error.to_string())? != mutation.observed {
            return Err(
                "Repository changed while preparing selected preview; no write was prepared".into(),
            );
        }
        let context = LocalContext {
            directory: repo.root.clone(),
            revision: status.head.clone().unwrap_or_else(|| "HEAD".into()),
            status,
            selected_path: anchor.new_path.clone().or_else(|| anchor.old_path.clone()),
            comparison: None,
            selected_anchor: Some(anchor),
            selected_lines: selected.to_vec(),
        };
        Ok(Self {
            command,
            context,
            mutation,
            preview,
        })
    }
    pub fn execute_stash_with_inspection_v1(
        &self,
        confirmed: bool,
        cancellation: CancellationToken,
        origin: std::sync::Arc<crate::git::OriginalLocalActorOriginV1>,
    ) -> Result<LocalOutcome, crate::git::StashAttemptFailureV1> {
        origin
            .recheck(&cancellation)
            .map_err(|error| crate::git::StashAttemptFailureV1::before(error.to_string()))?;
        let repo = repository(&self.context.directory, cancellation.clone())
            .map_err(crate::git::StashAttemptFailureV1::before)?;
        let success = repo.execute_stash_with_inspection_v1(&self.mutation, confirmed, origin)?;
        let (output, original_intent) = success.into_parts();
        let refresh_failure = |reason| {
            crate::git::StashAttemptFailureV1::after_success(reason, original_intent.clone())
        };
        // The successful write belongs to its genuine original source, not any
        // later checkout discovered at this pathname. Metadata checks add no
        // Git/gh/SQL observer and never turn the retained witness into approval.
        original_intent
            .origin()
            .recheck(&cancellation)
            .map_err(|error| refresh_failure(error.to_string()))?;
        let workspace = Workspace::load_with_cancellation(
            &self.context.directory,
            RepositoryMode::Overview,
            "HEAD",
            None,
            cancellation.clone(),
        )
        .map_err(refresh_failure)?
        .ok_or_else(|| {
            refresh_failure("Repository unavailable after successful stash command".into())
        })?;
        original_intent
            .origin()
            .recheck(&cancellation)
            .map_err(|error| refresh_failure(error.to_string()))?;
        let original = original_intent.origin();
        if !workspace.actor_origin.as_ref().is_some_and(|fresh| {
            fresh.source() == original.source()
                && fresh
                    .original_repository_v1()
                    .actual_read_runner_v1()
                    .same_instance(original.original_repository_v1().actual_read_runner_v1())
        }) {
            return Err(refresh_failure(
                "Original stash result refresh belongs to another source/producer".into(),
            ));
        }
        Ok(LocalOutcome {
            output: sanitize_terminal(&output),
            workspace,
        })
    }
    pub fn prepare(
        command: LocalCommand,
        context: &LocalContext,
        values: &BTreeMap<String, String>,
        cancellation: CancellationToken,
    ) -> Result<Self, String> {
        Self::prepare_selected(command, context, values, &BTreeMap::new(), cancellation)
    }

    /// Picker identities are explicit data, not a reconstruction of escaped editor labels.
    pub fn prepare_selected(
        command: LocalCommand,
        context: &LocalContext,
        values: &BTreeMap<String, String>,
        selections: &BTreeMap<String, crate::local_picker::LocalValue>,
        cancellation: CancellationToken,
    ) -> Result<Self, String> {
        if let Some(reason) = command.disabled_reason(context) {
            return Err(reason);
        }
        let repo = repository(&context.directory, cancellation.clone())?;
        let mut command_values = values.clone();
        for (field, selection) in selections {
            if !fields(command, context)
                .iter()
                .any(|candidate| candidate.key == field.as_str())
            {
                return Err("Selected choice does not belong to an operation field.".into());
            }
            let editor_text = selection.editor_text(command, field)?;
            if values.get(field) != Some(&editor_text) {
                return Err(format!("{field} was edited after choosing its target. Open a fresh picker or clear the typed choice."));
            }
            selection.validate(&repo)?;
            command_values.insert(field.clone(), editor_text);
        }
        let selected_worktree = selections
            .get("path")
            .and_then(|value| value.worktree_path());
        if selected_worktree.is_some() {
            // `operation` still validates the command/fields, but no lossy path reaches Git.
            command_values.insert("path".into(), "typed-worktree-target".into());
        }
        let mut operation = operation(command, context, &command_values)?;
        if let Some(path) = selected_worktree {
            operation = Operation::RemoveWorktree(path);
        }
        let current_status = repo.status().map_err(|error| error.to_string())?;
        let selected_conflict = current_status
            .entries
            .iter()
            .any(|entry| entry.conflict && Some(&entry.path) == context.selected_path.as_ref());
        let selected_untracked = current_status
            .entries
            .iter()
            .any(|entry| entry.untracked() && Some(&entry.path) == context.selected_path.as_ref());
        if let Operation::RemoveWorktree(path) = &operation {
            let linked = GitRepository::discover_with_cancellation(path, cancellation)
                .map_err(|error| error.to_string())?
                .ok_or("Linked worktree is unavailable.")?;
            if !linked.status().map_err(|error| error.to_string())?.clean() {
                return Err(
                    "Linked worktree is dirty. Save or stash its work before removing it.".into(),
                );
            }
        }
        let staged = if matches!(command, LocalCommand::Commit | LocalCommand::Amend) {
            Some(
                repo.compare(&Comparison::Staged)
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        let selected_diff = match command {
            LocalCommand::StageFile if selected_conflict || selected_untracked => None,
            LocalCommand::StageFile | LocalCommand::DiscardFile => Some(Comparison::Working),
            LocalCommand::UnstageFile => Some(Comparison::Staged),
            _ => None,
        }
        .map(|comparison| repo.compare(&comparison).map_err(|error| error.to_string()))
        .transpose()?;
        let mutation = if matches!(
            command,
            LocalCommand::StageSelection | LocalCommand::UnstageSelection
        ) {
            let rendered = context
                .comparison
                .as_ref()
                .ok_or("Open a working or staged diff first.")?;
            let anchor = context
                .selected_anchor
                .as_ref()
                .ok_or("Select a canonical diff line first.")?;
            repo.plan_selected_patch(
                rendered,
                anchor.file_index,
                &context.selected_lines,
                command == LocalCommand::UnstageSelection,
            )
            .map_err(|error| error.to_string())?
        } else {
            repo.plan(operation).map_err(|error| error.to_string())?
        };
        if let Some(staged) = &staged {
            if staged.observed != Some(mutation.observed) {
                return Err(
                    "Repository changed while preparing the staged preview. Refresh and try again."
                        .into(),
                );
            }
        }
        if selected_diff
            .as_ref()
            .is_some_and(|diff| diff.observed != Some(mutation.observed))
        {
            return Err("Repository changed while preparing the selected-file preview. Refresh and try again.".into());
        }
        let mut preview = preview_header_v2(command, &mutation);
        if mutation.destructive {
            preview.push("Warning: this can replace or remove existing history/files. Inspect the target before confirming.".into());
        }
        if selected_conflict && command == LocalCommand::StageFile {
            preview.push("Stages the complete resolved working file and removes its base/ours/theirs conflict stages. Inspect the working content before confirming.".into());
            if let Some(path) = &context.selected_path {
                if conflict_markers_v1(&repo.root.join(path.as_os_str())) {
                    preview.push("Warning: the working file still contains <<<<<<< / ======= / >>>>>>> conflict markers. Choose a version or edit them out first unless they are intentional.".into());
                }
            }
        }
        if selected_untracked && command == LocalCommand::StageFile {
            let path = context
                .selected_path
                .as_ref()
                .ok_or("Select an exact new file first.")?;
            let absolute = repo.root.join(path.as_os_str());
            let metadata =
                std::fs::symlink_metadata(&absolute).map_err(|error| error.to_string())?;
            preview.push(format!(
                "New untracked file: {} — the complete file is staged after confirmation",
                path.display()
            ));
            if metadata.file_type().is_symlink() {
                use std::os::unix::ffi::OsStrExt;
                let target = std::fs::read_link(&absolute).map_err(|error| error.to_string())?;
                preview.push(format!(
                    "Symbolic link target: {} — referenced file content is not staged",
                    RawPath::new(target.as_os_str().as_bytes().to_vec()).display()
                ));
            } else {
                match crate::workspace::read_working_file(&repo.root, path, 2 * 1024 * 1024) {
                    Ok(content) if content.contains(&0) || std::str::from_utf8(&content).is_err() => preview.push(format!("Binary new file: {} bytes; complete file will be staged", content.len())),
                    Ok(content) => {
                        // A final newline ends the last line; it is not an extra empty line.
                        let body = content.strip_suffix(b"\n").unwrap_or(&content);
                        if content.is_empty() {
                            preview.push("Empty new file: 0 bytes".into());
                        } else {
                            preview.extend(body.split(|byte| *byte == b'\n').take(120).enumerate().map(|(index, line)| format!("{:>5} {}", index + 1, crate::git::display_bytes(line))));
                            if body.split(|byte| *byte == b'\n').count() > 120 { preview.push("Content preview stops after 120 lines; the complete file is staged.".into()); }
                            if body.len() == content.len() { preview.push("\\ No newline at end of file".into()); }
                        }
                    }
                    Err(error) if error == "Search file viewer limit is 2 MiB; file was not read" => preview.push(format!("Large new file: {} bytes. Content exceeds the 2 MiB viewer limit; complete file will be staged only after confirmation.", metadata.len())),
                    Err(error) => return Err(error),
                }
            }
        }
        match &mutation.operation {
            Operation::StashApply { id, .. } | Operation::StashDrop { id } => {
                let resolved = repo.stashes().map_err(|error| error.to_string())?.into_iter().find(|stash| stash.id == *id || stash.selector == *id).ok_or("Selected stash is no longer available.")?;
                preview.push(format!("Exact stash commit: {} — list positions are not stable identities", resolved.id));
            }
            Operation::RemoveWorktree(path) | Operation::AddWorktree { path, .. } | Operation::AddWorktreeBranchAtV9 { path, .. } => {
                use std::os::unix::ffi::OsStrExt;
                preview.push(format!("Exact worktree directory: {}", RawPath::new(path.as_os_str().as_bytes().to_vec()).display()));
            }
            Operation::Stage(paths) | Operation::Unstage(paths) | Operation::DiscardFiles(paths) => {
                for path in paths { preview.push(format!("Exact file: {}", path.display())); }
            }
            Operation::ResolveConflict { path, stage } => preview.push(format!("Exact file: {} — replaces working bytes with stage {stage}; stage separately after inspection.", path.display())),
            Operation::ForcePushLease { expected_remote, .. } => preview.push(format!("Will refuse if remote branch is not exactly {expected_remote}. Never uses unrestricted --force.")),
            Operation::ApplyPatch { .. } => append_selected_patch_preview_v2(&mut preview,&mutation),
            _ => {}
        }
        preview.extend(branch_sync_preview_v1(
            &repo,
            &mutation.operation,
            &current_status,
        ));
        if let Some(staged) = staged {
            preview.push("Exact staged content for the next commit:".into());
            if staged.diff.files.is_empty() {
                preview.push("No staged file changes.".into());
            }
            preview.extend(
                String::from_utf8_lossy(&staged.patch)
                    .lines()
                    .map(sanitize_terminal),
            );
        }
        if let (Some(mut comparison), Some(path)) = (selected_diff, &context.selected_path) {
            comparison.diff.files.retain(|file| {
                file.old_path.as_deref() == Some(path.0.as_slice())
                    || file.new_path.as_deref() == Some(path.0.as_slice())
            });
            preview.push("Current selected-file changes:".into());
            if comparison.diff.files.is_empty() {
                preview.push("No tracked text patch. New files, symlinks, binary files or unmerged entries are staged as whole files.".into());
            }
            preview.extend(
                diff_rows(&comparison.diff, DiffLayout::Unified, true)
                    .into_iter()
                    .map(|row| row.left.label),
            );
        }
        if let Operation::ResolveConflict { path, stage } = &mutation.operation {
            let selected = repo
                .conflict_stages()
                .map_err(|error| error.to_string())?
                .into_iter()
                .find(|item| item.path == *path && item.stage == *stage)
                .ok_or("Selected conflict version is no longer available.")?;
            preview.push(format!("Immutable conflict object: {}", selected.id));
            let content = repo
                .object(&selected.id)
                .map_err(|error| error.to_string())?;
            if content.contains(&0) {
                preview.push("Binary conflict version — replaces the full file bytes.".into());
            } else {
                preview.extend(
                    String::from_utf8_lossy(&content)
                        .lines()
                        .take(120)
                        .map(sanitize_terminal),
                );
            }
        }
        if repo.fingerprint().map_err(|error| error.to_string())? != mutation.observed {
            return Err("Repository changed while preparing the operation preview. Refresh and confirm a new preview.".into());
        }
        Ok(Self {
            command,
            context: context.clone(),
            mutation,
            preview,
        })
    }
    pub fn execute(
        &self,
        confirmed: bool,
        cancellation: CancellationToken,
    ) -> Result<LocalOutcome, String> {
        let repo = repository(&self.context.directory, cancellation.clone())?;
        let output = repo.execute(&self.mutation, confirmed).map_err(|error| {
            let error = error.to_string();
            let summary = explain_git_failure_v1(self.command, &error)
                .map(|summary| format!("{summary}\n"))
                .unwrap_or_default();
            format!("{summary}{error}\nRefresh repository state before another write; do not retry an uncertain operation blindly.")
        })?;
        let mut workspace = Workspace::load_with_cancellation(
            &self.context.directory,
            refreshed_mode(self.command, &self.context),
            "HEAD",
            None,
            cancellation,
        )?
        .ok_or("Repository is unavailable after the operation.")?;
        if matches!(
            self.command,
            LocalCommand::BisectStart
                | LocalCommand::BisectGood
                | LocalCommand::BisectBad
                | LocalCommand::BisectSkip
                | LocalCommand::BisectReset
        ) {
            let explanation = if self.command == LocalCommand::BisectReset {
                "Bug search ended. Git restored the starting saved version/branch."
            } else if output.contains("first bad commit") {
                "Culprit found: the current saved version is the first bad commit. Inspect it, then End bug search to restore the starting branch."
            } else {
                "Bug search is paused at the current saved version. Inspect/test it, then Mark version good, Mark version bad, or Skip untestable version. End bug search restores the starting branch."
            };
            workspace.rows.push(DisplayRow {
                label: explanation.into(),
                role: crate::repository_view::SemanticRole::Reference,
                action: None,
            });
            workspace
                .rows
                .extend(
                    output
                        .lines()
                        .filter(|line| !line.is_empty())
                        .take(10)
                        .map(|line| DisplayRow {
                            label: sanitize_terminal(line),
                            role: crate::repository_view::SemanticRole::Normal,
                            action: None,
                        }),
                );
        }
        Ok(LocalOutcome {
            output: sanitize_terminal(&output),
            workspace,
        })
    }
}
#[derive(Clone, Debug)]
pub struct LocalOutcome {
    pub output: String,
    pub workspace: Workspace,
}

/// Background-only inventory. These typed identities can seed selectors without parsing labels.
#[derive(Clone, Debug)]
pub struct LocalInventory {
    pub directory: PathBuf,
    pub references: Vec<Reference>,
    pub remotes: Vec<Remote>,
    pub stashes: Vec<Stash>,
    pub worktrees: Vec<Worktree>,
    pub reflog: Vec<ReflogEntry>,
}
impl LocalInventory {
    pub fn load(directory: &Path, cancellation: CancellationToken) -> Result<Self, String> {
        let repo = repository(directory, cancellation)?;
        Ok(Self {
            directory: repo.root.clone(),
            references: repo.references().map_err(|error| error.to_string())?,
            remotes: repo.remotes().map_err(|error| error.to_string())?,
            stashes: repo.stashes().map_err(|error| error.to_string())?,
            worktrees: repo.worktrees().map_err(|error| error.to_string())?,
            reflog: repo.reflog(80).map_err(|error| error.to_string())?,
        })
    }
    pub fn rows(&self) -> Vec<String> {
        let mut rows = vec![
            "Local inventory — browsing does not change Git state".into(),
            "Branches, tags and tracking references:".into(),
        ];
        rows.extend(self.references.iter().take(256).map(|reference| {
            format!(
                "{} {} → {}{}",
                if reference.current { "●" } else { "○" },
                sanitize_terminal(&reference.name),
                reference.id,
                reference
                    .upstream
                    .as_ref()
                    .map(|name| format!(" • tracks {}", sanitize_terminal(name)))
                    .unwrap_or_default()
            )
        }));
        rows.push("Configured remotes:".into());
        rows.extend(self.remotes.iter().take(32).map(|remote| {
            format!(
                "{} • fetch {} • push {}",
                sanitize_terminal(&remote.name),
                redact_remote_credentials(&remote.fetch_url),
                redact_remote_credentials(&remote.push_url)
            )
        }));
        rows.push("Saved stashes — use exact SHA, not a shifting stash position:".into());
        rows.extend(self.stashes.iter().take(80).map(|stash| {
            format!(
                "{} • {} • {}",
                stash.id,
                sanitize_terminal(&stash.selector),
                sanitize_terminal(&stash.subject)
            )
        }));
        rows.push("Linked worktrees:".into());
        rows.extend(self.worktrees.iter().take(64).map(|tree| {
            format!(
                "{} • {}{}{}",
                sanitize_terminal(&tree.path.to_string_lossy()),
                tree.branch.as_deref().unwrap_or("detached"),
                if tree.locked { " [locked]" } else { "" },
                if tree.prunable {
                    " [missing/prunable]"
                } else {
                    ""
                }
            )
        }));
        rows.push("Recent local movements — reflog is local and can expire:".into());
        rows.extend(self.reflog.iter().map(|entry| {
            format!(
                "{} • {} • {}",
                entry.id,
                sanitize_terminal(&entry.selector),
                sanitize_terminal(&entry.subject)
            )
        }));
        rows
    }
}

/// Plain-language branch/remote state for branch and sync previews. Reads only local refs:
/// counts describe the last fetch, never an implicit network probe.
pub(crate) fn branch_sync_preview_v1(
    repo: &GitRepository,
    operation: &Operation,
    status: &RepoStatus,
) -> Vec<String> {
    let mut lines = Vec::new();
    let exists = |name: &str| repo.resolve(name).ok();
    let counts = |base: &str, head: &str| repo.ahead_behind(base, head).ok();
    let plural = |count: usize| if count == 1 { "" } else { "s" };
    let upstream_line = |branch: &str| {
        let reference = repo.references().ok().and_then(|references| {
            references
                .into_iter()
                .find(|reference| reference.raw_name == format!("refs/heads/{branch}").as_bytes())
        });
        match reference.and_then(|reference| reference.upstream) {
            Some(upstream) => format!(
                "Local branch {branch} tracks {}",
                upstream.trim_start_matches("refs/remotes/")
            ),
            None => format!("Local branch {branch} has no upstream (tracking branch) yet"),
        }
    };
    match operation {
        Operation::Fetch { remote } => {
            lines.push(format!("Fetch downloads new commits from {remote} and updates only {remote}/* tracking refs. Your branches and working files do not change."));
            if let (Some(branch), Some(upstream)) = (&status.branch, &status.upstream) {
                lines.push(format!(
                    "Before fetch: {branch} is {} ahead and {} behind {upstream} (as of the last fetch).",
                    status.ahead, status.behind
                ));
            }
        }
        Operation::Pull { remote, branch } | Operation::PullWithStrategy { remote, branch, .. } => {
            let strategy = match operation {
                Operation::PullWithStrategy { strategy, .. } => *strategy,
                _ => PullStrategy::FastForwardOnly,
            };
            let current = status.branch.as_deref().unwrap_or("(detached HEAD)");
            let tracking = format!("refs/remotes/{remote}/{branch}");
            lines.push(format!(
                "Pull fetches {remote} {branch}, then updates the current branch {current} and its working files."
            ));
            match exists(&tracking).and_then(|_| counts(&tracking, "HEAD")) {
                Some((ahead, behind)) => {
                    lines.push(format!("As of the last fetch: {ahead} local commit{} not on {remote}/{branch}; {behind} commit{} on {remote}/{branch} not here.", plural(ahead), plural(behind)));
                    if ahead > 0 && behind > 0 {
                        lines.push(format!("DIVERGED: {current} and {remote}/{branch} each have commits the other lacks."));
                        lines.push(match strategy {
                            PullStrategy::FastForwardOnly => "Fast-forward only will refuse this pull and change nothing. Choose merge (adds a merge commit) or rebase (replays your local commits on top).".into(),
                            PullStrategy::Merge => "Merge combines both sides in a new merge commit. Conflicts stop the merge for resolution; then Continue operation or Abort operation.".into(),
                            PullStrategy::Rebase => format!("Rebase replays your {ahead} local commit{} on top of {remote}/{branch}. Conflicts stop for resolution; then Continue, Skip or Abort operation.", plural(ahead)),
                        });
                    } else if behind > 0 {
                        lines.push(format!("BEHIND: pull brings in {behind} commit{} as a fast-forward.", plural(behind)));
                    } else {
                        lines.push("Up to date with the last fetch; pull may still bring in newer remote commits.".into());
                    }
                }
                None => lines.push(format!(
                    "No local copy of {remote}/{branch} yet; pull fetches it first. Ahead/behind is unknown until then."
                )),
            }
        }
        Operation::Push {
            remote,
            branch,
            set_upstream,
        } => {
            let tracking = format!("refs/remotes/{remote}/{branch}");
            lines.push(upstream_line(branch));
            match exists(&tracking).and_then(|_| counts(&tracking, &format!("refs/heads/{branch}")))
            {
                None => {
                    lines.push(format!("FIRST PUSH: creates branch {branch} on {remote}."));
                    if !set_upstream {
                        lines.push(format!("Set tracking branch? true is recommended so later pull/push and ahead/behind use {remote}/{branch}."));
                    }
                }
                Some((ahead, behind)) => {
                    lines.push(format!("As of the last fetch: sends {ahead} commit{}; {remote}/{branch} has {behind} commit{} not in {branch}.", plural(ahead), plural(behind)));
                    if behind > 0 {
                        lines.push(format!("REMOTE AHEAD: a normal push will be rejected and change nothing. Pull {remote} {branch} (merge or rebase) first. This push never forces."));
                    } else if ahead == 0 {
                        lines.push(format!(
                            "Nothing new to send: {remote}/{branch} already has {branch}."
                        ));
                    }
                }
            }
        }
        Operation::ForcePushLease { remote, branch, .. } => {
            let tracking = format!("refs/remotes/{remote}/{branch}");
            if let Some((ahead, behind)) =
                exists(&tracking).and_then(|_| counts(&tracking, &format!("refs/heads/{branch}")))
            {
                lines.push(format!("REPLACES remote history: {behind} commit{} on {remote}/{branch} (as of the last fetch) will no longer be on the remote branch; {ahead} local commit{} are sent.", plural(behind), plural(ahead)));
            }
        }
        Operation::SetUpstream { branch, upstream } => {
            lines.push(upstream_line(branch));
            if let Some((ahead, behind)) = counts(upstream, &format!("refs/heads/{branch}")) {
                lines.push(format!("After this change: {branch} is {ahead} ahead and {behind} behind {upstream}. Only configuration changes; no commits or files move."));
            }
        }
        Operation::SwitchBranch(name) => {
            if exists(&format!("refs/heads/{name}")).is_some() {
                lines.push(upstream_line(name));
            } else if let Some(remote) = repo.remotes().ok().and_then(|remotes| {
                remotes.into_iter().find(|remote| {
                    exists(&format!("refs/remotes/{}/{name}", remote.name)).is_some()
                })
            }) {
                lines.push(format!("No local branch {name}: Git creates it from {}/{name} and sets it as upstream.", remote.name));
            }
        }
        Operation::DeleteBranch { name, force } => {
            if let Some(id) = exists(&format!("refs/heads/{name}")) {
                lines.push(format!("Recovery: branch {name} currently points at {id}. Create branch or Create rescue branch with this saved version restores it."));
                if let Some((unmerged, _)) = counts("HEAD", &format!("refs/heads/{name}")) {
                    // Git also allows a normal delete when the branch is contained in its upstream.
                    let saved_upstream = repo
                        .references()
                        .ok()
                        .and_then(|references| {
                            references.into_iter().find(|reference| {
                                reference.raw_name == format!("refs/heads/{name}").as_bytes()
                            })
                        })
                        .and_then(|reference| reference.upstream)
                        .filter(|upstream| {
                            counts(upstream, &format!("refs/heads/{name}"))
                                .is_some_and(|(ahead, _)| ahead == 0)
                        });
                    if unmerged > 0 {
                        let effect = if *force {
                            " Forced delete removes the branch anyway; keep the recovery SHA above."
                                .to_owned()
                        } else if let Some(upstream) = saved_upstream {
                            format!(" They are saved on its upstream {}, so a normal delete is allowed.", upstream.trim_start_matches("refs/remotes/"))
                        } else {
                            " A normal delete will refuse; choose Delete unmerged history? true only after saving that work.".to_owned()
                        };
                        lines.push(format!("UNMERGED: {name} has {unmerged} commit{} not in the current branch.{effect}", plural(unmerged)));
                    } else {
                        lines.push(format!("{name} is fully contained in the current branch."));
                    }
                }
            }
            if status.branch.as_deref() == Some(name.as_str()) {
                lines.push(format!(
                    "{name} is the current branch; switch to another branch before deleting it."
                ));
            }
            if let Some(tree) = repo.worktrees().ok().and_then(|trees| {
                trees.into_iter().skip(1).find(|tree| {
                    tree.branch.as_deref() == Some(format!("refs/heads/{name}").as_str())
                })
            }) {
                lines.push(format!(
                    "{name} is checked out in linked worktree {}; Git will refuse to delete it.",
                    sanitize_terminal(&tree.path.to_string_lossy())
                ));
            }
        }
        _ => {}
    }
    lines
        .into_iter()
        .map(|line| sanitize_terminal(&line))
        .collect()
}

/// A short plain-language reason placed above the raw Git output for common failures.
pub(crate) fn explain_git_failure_v1(command: LocalCommand, error: &str) -> Option<&'static str> {
    let has = |needle: &str| error.contains(needle);
    Some(if has("Previous write outcome is unknown") {
        "An earlier Git write in this session failed or was interrupted. Press r Refresh to re-read the repository, then preview again."
    } else if has("Committer identity unknown") || has("Please tell me who you are") {
        "Git does not know your name and email. Set user.name and user.email with git config; if an operation is now in progress, Continue operation or Abort operation afterwards."
    } else if has("CONFLICT") || has("Merge conflict") || has("could not apply") {
        "CONFLICT: the operation stopped on conflicting changes. Your work is kept: open the conflicted files, choose versions and stage them, then Continue operation, or Abort operation to return to the previous state."
    } else if has("Diverging branches can't be fast-forwarded")
        || has("Not possible to fast-forward")
        || (has("fatal:") && has("fast-forward"))
    {
        "Pull refused: local and remote histories have diverged, and fast-forward only changes nothing. Pull again with the merge or rebase strategy."
    } else if has("[rejected]") || has("non-fast-forward") || has("fetch first") {
        "Push rejected: the remote branch has commits you do not have. The remote was not changed. Pull (merge or rebase), then push again."
    } else if has("stale info") {
        "Force push refused: the remote branch no longer matches the expected commit. Fetch and review it first."
    } else if has("Authentication failed")
        || has("Permission denied")
        || has("could not read Username")
    {
        "The remote refused the credentials. Nothing was changed by this command; check your Git credentials for this remote."
    } else if has("does not appear to be a git repository")
        || has("Could not read from remote")
        || has("Could not resolve host")
    {
        "The remote could not be reached. Nothing was changed by this command; check the remote URL and network."
    } else if has("couldn't find remote ref") {
        "That branch does not exist on the remote. Check the remote branch name, or push it first."
    } else if has("not fully merged") {
        "Branch not deleted: it has commits not merged into the current branch. Choose Delete unmerged history? true only after saving that work."
    } else if has("checked out at") || has("used by worktree") {
        "Branch is checked out in this or another worktree. Switch that worktree to another branch first."
    } else if has("already exists") {
        "That name is already used. Choose a different name; nothing was changed."
    } else if has("would be overwritten") {
        "Local changes would be overwritten. Commit or stash them first; your files were not changed."
    } else if has("invalid upstream")
        || (has("does not exist") && command == LocalCommand::SetUpstream)
    {
        "That tracking branch does not exist locally. Fetch the remote first, then set upstream."
    } else {
        return None;
    })
}

fn repository(directory: &Path, cancellation: CancellationToken) -> Result<GitRepository, String> {
    GitRepository::discover_with_cancellation(directory, cancellation)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Local Git repository is unavailable.".into())
}
fn required(values: &BTreeMap<String, String>, key: &str) -> Result<String, String> {
    let value = values.get(key).map(String::as_str).unwrap_or("").trim();
    if value.is_empty() {
        return Err(format!("{key} is required."));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{key} contains an unsupported control character."));
    }
    Ok(value.into())
}
fn boolean(values: &BTreeMap<String, String>, key: &str) -> Result<bool, String> {
    match values.get(key).map(|value| value.trim()).unwrap_or("false") {
        "false" => Ok(false),
        "true" => Ok(true),
        _ => Err(format!("{key} must be true or false.")),
    }
}
fn operation(
    command: LocalCommand,
    context: &LocalContext,
    values: &BTreeMap<String, String>,
) -> Result<Operation, String> {
    use LocalCommand::*;
    let value = |key| required(values, key);
    let path = || {
        context
            .selected_path
            .clone()
            .ok_or_else(|| "Select an exact file first.".to_string())
    };
    Ok(match command {
        StageFile => Operation::Stage(vec![path()?]),
        UnstageFile => Operation::Unstage(vec![path()?]),
        StageSelection | UnstageSelection => Operation::ApplyPatch {
            patch: vec![],
            reverse: command == UnstageSelection,
        },
        Commit | Amend => {
            let message = values.get("message").cloned().unwrap_or_default();
            if message.trim().is_empty() || message.contains('\0') {
                return Err("A nonempty full commit message without NUL is required.".into());
            }
            Operation::Commit {
                message,
                amend: command == Amend,
            }
        }
        CreateBranch => Operation::CreateBranch {
            name: value("name")?,
            revision: value("revision")?,
        },
        RenameBranch => Operation::RenameBranch {
            old: value("old")?,
            new: value("new")?,
        },
        SwitchBranch => Operation::SwitchBranch(value("name")?),
        DeleteBranch => Operation::DeleteBranch {
            name: value("name")?,
            force: boolean(values, "force")?,
        },
        CreateTag => Operation::CreateTag {
            name: value("name")?,
            revision: value("revision")?,
        },
        DeleteTag => Operation::DeleteTag(value("name")?),
        SetUpstream => Operation::SetUpstream {
            branch: value("branch")?,
            upstream: value("upstream")?,
        },
        Fetch => Operation::Fetch {
            remote: value("remote")?,
        },
        Pull => Operation::PullWithStrategy {
            remote: value("remote")?,
            branch: value("branch")?,
            strategy: match value("strategy")?.as_str() {
                "ff-only" => PullStrategy::FastForwardOnly,
                "merge" => PullStrategy::Merge,
                "rebase" => PullStrategy::Rebase,
                _ => return Err("strategy must be ff-only, merge, or rebase.".into()),
            },
        },
        Push => Operation::Push {
            remote: value("remote")?,
            branch: value("branch")?,
            set_upstream: boolean(values, "set_upstream")?,
        },
        ForcePushLease => Operation::ForcePushLease {
            remote: value("remote")?,
            branch: value("branch")?,
            expected_remote: value("expected_remote")?,
        },
        CreatePullRequest => {
            return Err(
                "Create pull request is previewed with GitHub, not as a Git operation.".into(),
            )
        }
        StashSave => Operation::StashSave {
            message: values.get("message").cloned().unwrap_or_default(),
            untracked: boolean(values, "untracked")?,
        },
        StashApply | StashPop => Operation::StashApply {
            id: value("id")?,
            pop: command == StashPop,
        },
        StashDrop => Operation::StashDrop { id: value("id")? },
        AddWorktree => Operation::AddWorktree {
            path: PathBuf::from(value("path")?),
            branch: value("branch")?,
        },
        RemoveWorktree => Operation::RemoveWorktree(PathBuf::from(value("path")?)),
        Merge => Operation::Merge(value("revision")?),
        Rebase => Operation::Rebase(value("revision")?),
        CherryPick => Operation::CherryPick(value("revision")?),
        Revert => Operation::Revert(value("revision")?),
        Continue => Operation::Continue,
        Skip => Operation::Skip,
        Abort => Operation::Abort,
        ResolveConflict => Operation::ResolveConflict {
            path: path()?,
            stage: conflict_stage_v1(&value("stage")?)?,
        },
        RecoverBranch => Operation::RecoverBranch {
            name: value("name")?,
            revision: value("revision")?,
        },
        Reset => Operation::Reset {
            revision: value("revision")?,
            mode: match value("mode")?.as_str() {
                "soft" => ResetMode::Soft,
                "mixed" => ResetMode::Mixed,
                "hard" => ResetMode::Hard,
                _ => return Err("mode must be soft, mixed, or hard.".into()),
            },
        },
        DiscardFile => Operation::DiscardFiles(vec![path()?]),
        BisectStart => Operation::BisectStart {
            good: value("good")?,
            bad: value("bad")?,
        },
        BisectGood | BisectBad => Operation::BisectMark {
            good: command == BisectGood,
        },
        BisectSkip => Operation::BisectSkip,
        BisectReset => Operation::BisectReset,
    })
}

/// Bounded check for unresolved conflict markers at line starts in a regular file.
pub(crate) fn conflict_markers_v1(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() > 4 * 1024 * 1024 {
        return false;
    }
    let content = std::fs::read(path).unwrap_or_default();
    let mut opened = false;
    for line in content.split(|byte| *byte == b'\n') {
        if line.starts_with(b"<<<<<<< ") || line == b"<<<<<<<" {
            opened = true;
        } else if opened && (line.starts_with(b">>>>>>> ") || line == b">>>>>>>") {
            return true;
        }
    }
    false
}

/// Conflict version names map to Git index stages; numeric stages stay accepted.
pub(crate) fn conflict_stage_v1(value: &str) -> Result<u8, String> {
    match value.trim() {
        "base" | "1" => Ok(1),
        "ours" | "2" => Ok(2),
        "theirs" | "3" => Ok(3),
        _ => Err("Version must be ours, theirs or base.".into()),
    }
}

/// Explicit error-recovery refresh. A local refresh cannot prove an uncertain remote push outcome.
pub fn reconcile(directory: &Path, cancellation: CancellationToken) -> Result<Workspace, String> {
    let repo = repository(directory, cancellation.clone())?;
    repo.reconcile_write().map_err(|error| error.to_string())?;
    Workspace::load_with_cancellation(
        directory,
        RepositoryMode::Overview,
        "HEAD",
        None,
        cancellation,
    )?
    .ok_or_else(|| "Repository is unavailable.".into())
}

pub fn redact_remote_credentials(label: &str) -> String {
    crate::git::display_remote_url(label)
}
