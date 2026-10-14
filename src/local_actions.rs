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
    pub const ALL: [(Self, &'static str); 39] = [
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
                "false",
                &["false", "true"],
            ),
        ],
        ForcePushLease => vec![
            field("remote", "Configured remote", "origin"),
            field("branch", "Local branch", branch),
            field("expected_remote", "Exact expected remote commit SHA", ""),
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
            "Version: base=1, ours=2, theirs=3",
            "2",
            &["1", "2", "3"],
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
impl LocalPlan {
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
        let mut preview = vec![
            format!("{} — no changes until confirmation", command.label()),
            format!("Target: {}", redact_remote_credentials(&mutation.target)),
            format!("Git effect: {}", sanitize_terminal(&mutation.effect)),
        ];
        if mutation.destructive {
            preview.push("Warning: this can replace or remove existing history/files. Inspect the target before confirming.".into());
        }
        if selected_conflict && command == LocalCommand::StageFile {
            preview.push("Stages the complete resolved working file and removes its base/ours/theirs conflict stages. Inspect the working content before confirming.".into());
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
                        preview.extend(content.split(|byte| *byte == b'\n').take(120).enumerate().map(|(index, line)| format!("{:>5} {}", index + 1, crate::git::display_bytes(line))));
                        if content.split(|byte| *byte == b'\n').count() > 120 { preview.push("Content preview stops after 120 lines; the complete file is staged.".into()); }
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
            Operation::RemoveWorktree(path) | Operation::AddWorktree { path, .. } => {
                use std::os::unix::ffi::OsStrExt;
                preview.push(format!("Exact worktree directory: {}", RawPath::new(path.as_os_str().as_bytes().to_vec()).display()));
            }
            Operation::Stage(paths) | Operation::Unstage(paths) | Operation::DiscardFiles(paths) => {
                for path in paths { preview.push(format!("Exact file: {}", path.display())); }
            }
            Operation::ResolveConflict { path, stage } => preview.push(format!("Exact file: {} — replaces working bytes with stage {stage}; stage separately after inspection.", path.display())),
            Operation::ForcePushLease { expected_remote, .. } => preview.push(format!("Will refuse if remote branch is not exactly {expected_remote}. Never uses unrestricted --force.")),
            Operation::ApplyPatch { patch, .. } => preview.extend(String::from_utf8_lossy(patch).lines().map(sanitize_terminal)),
            _ => {}
        }
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
        let output = repo.execute(&self.mutation, confirmed).map_err(|error| format!("{error}\nRefresh repository state before another write; do not retry an uncertain operation blindly."))?;
        let mut workspace = Workspace::load_with_cancellation(
            &self.context.directory,
            RepositoryMode::Overview,
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
            stage: value("stage")?
                .parse()
                .map_err(|_| "stage must be 1, 2, or 3.")?,
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
