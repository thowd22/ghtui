//! Native Workflow→local matching is read-only and not remote association.
//! Closed post-read commands reuse the installed-Git spec/parser boundary;
//! no raw arguments, runner, provider or CommandSpec can escape this module.
use super::*;
use crate::{
    providers::{
        gh::{
            GithubError, GithubErrorKind, GithubProvider, GithubResult, ScopedActionsWorkflowV2,
            WorkflowEditPhaseV9, WorkflowGitRoleV9, WorkflowPostCaptureV9,
        },
        remote_git_read_window_v1::RemoteGitReadWindowV1,
    },
    scoped_actions_workflow_edit_v9::WorkflowEditIntentV9,
    storage::{Cache, RemoteMappingCandidateV1},
};
use std::time::Instant;
mod environment_v9;
mod file_v9;
pub(crate) use environment_v9::{WorkflowGitEnvironmentV9, WorkflowGitReadEnvironmentV9};
mod post_v9;
mod remote_v9;
pub(crate) use remote_v9::{
    WorkflowPreparedPushV9, WorkflowRemoteGitCommandV9, WorkflowRemoteGitObservationV9,
};
mod worktree_v9;
mod write_v9;
pub(crate) use file_v9::WorkflowWorkingFileV9;
pub(crate) use post_v9::WorkflowObservedCommitV9;
pub(crate) use worktree_v9::{
    WorkflowPreparedWorktreeV9, WorkflowWorktreeMaskV9, WorkflowWorktreePurposeV9,
    WorkflowWorktreeReadCommandV9, WorkflowWorktreeTargetV9, WorkflowWorktreeWriteCommandV9,
};
pub(crate) use write_v9::{
    WorkflowGitWriteCommandV9, WorkflowPreparedCommitV9, WorkflowPreparedStageV9,
};

/// Actual matching bytes plus a current root/locator observation. Private
/// construction; no saved path/name/ID constructor and no remote permission.
pub(crate) struct WorkflowLocalBindingV9 {
    intent: Arc<WorkflowEditIntentV9>,
    mapping: Arc<RemoteMappingCandidateV1>,
    origin: Arc<OriginalLocalActorOriginV1>,
    root_descriptor: std::fs::File,
    parent: Option<Arc<WorkflowLocalBindingV9>>,
    isolated_branch: Option<String>,
    config: Vec<u8>,
    mode: String,
    blob: String,
}
impl fmt::Debug for WorkflowLocalBindingV9 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkflowLocalBindingV9(<matching actual bytes; not association/approval>)")
    }
}
fn unavailable() -> GitError {
    GitError(
        "Original workflow/local mapping/content changed or unavailable; no fallback/local write"
            .into(),
    )
}
fn post_unavailable() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Original workflow post-read phase/root/token/cursor unavailable; no retry or private diagnostics".into(),
    }
}
impl WorkflowLocalBindingV9 {
    pub(crate) fn check_cleanup_origin_v9(&self, cancel: &CancellationToken) -> GitResult<()> {
        self.check_origin_v9(cancel)
    }
    pub(crate) fn prepare_cleanup_mapping_v9(
        &self,
        tx: &rusqlite::Transaction<'_>,
        heap: &crate::providers::gh::WorkflowJsonLedgerV9,
    ) -> anyhow::Result<crate::storage::workflow_cleanup_v9::WorkflowRawRowsV9> {
        self.mapping.prepare_cleanup_v9(tx, heap)
    }
    pub(crate) fn execute_worktree_v9(
        self: &Arc<Self>,
        request: &crate::scoped_actions_workflow_edit_v9::WorkflowWorktreeConfirmV9,
        provider: &GithubProvider,
        database: &Path,
    ) -> GithubResult<crate::scoped_actions_workflow_edit_v9::WorkflowWorktreeOutcomeV9> {
        use std::sync::TryLockError;
        let check = || request.check_original_v9(self);
        check()?;
        let locks = MUTATION_LOCKS.get_or_init(Mutex::default);
        let mut map = loop {
            check()?;
            match locks.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        };
        let lock = map
            .entry(self.origin.original_repository_v1().common_dir.clone())
            .or_default()
            .clone();
        drop(map);
        let guard = loop {
            check()?;
            match lock.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        };
        request.execute_locked_v9(
            provider,
            database,
            &WorkflowMutationLockV9 {
                binding: self,
                _guard: &guard,
            },
        )
    }
    fn check_origin_v9(&self, cancel: &CancellationToken) -> GitResult<()> {
        use std::os::unix::fs::MetadataExt;
        self.origin.recheck(cancel)?;
        let root = self.root_descriptor.metadata()?;
        let fresh = file_v9::root_descriptor_v9(&self.origin)?.metadata()?;
        if !root.is_dir()
            || (root.dev(), root.ino()) != self.origin.source().root_file_identity_v3()
            || (fresh.dev(), fresh.ino()) != (root.dev(), root.ino())
        {
            return Err(unavailable());
        }
        if let Some(parent) = &self.parent {
            if parent.parent.is_some() {
                return Err(unavailable());
            }
            parent.check_origin_v9(cancel)?;
        }
        Ok(())
    }
    pub(crate) fn current_repository_v9(
        &self,
        window: RemoteGitReadWindowV1,
        cancel: &CancellationToken,
    ) -> GitResult<GitRepository> {
        window.check(cancel).map_err(|_| unavailable())?;
        self.check_origin_v9(cancel)?;
        if !window
            .actual_runner_v1()
            .same_instance(self.origin.original_repository_v1().actual_read_runner_v1())
        {
            return Err(unavailable());
        }
        let original = self.origin.original_repository_v1();
        let context = WorkflowGitReadEnvironmentV9::capture_v9(
            &self.intent,
            &original.root,
            &window,
            cancel,
        )?;
        let window = window.with_workflow_environment_v9(context);
        let current = GitRepository::discover_with_remote_read_window_v1(
            &original.root,
            cancel.clone(),
            window,
        )?
        .ok_or_else(unavailable)?;
        if current.root != original.root
            || current.git_dir != original.git_dir
            || current.common_dir != original.common_dir
            || current.bare
            || !current
                .actual_read_runner_v1()
                .same_instance(original.actual_read_runner_v1())
        {
            return Err(unavailable());
        }
        self.check_origin_v9(cancel)?;
        Ok(current)
    }
    pub(crate) fn check_save_repository_v9(
        &self,
        repository: &GitRepository,
    ) -> GitResult<(WorkspaceFingerprint, Vec<u8>)> {
        self.check_fresh_plan_repository_v9(repository)
            .map(|(_, fingerprint, config)| (fingerprint, config))
    }
    pub(crate) fn check_fresh_plan_repository_v9(
        &self,
        repository: &GitRepository,
    ) -> GitResult<(RepoStatus, WorkspaceFingerprint, Vec<u8>)> {
        let status = repository.status()?;
        let path = RawPath(self.intent.definition().workflow.path.as_bytes().to_vec());
        if !self.branch_matches_v9(&status)
            || status.head.as_deref() != Some(self.intent.definition().commit_sha.as_str())
            || status.operation.is_some()
            || status.has_history_overrides()
            || status.conflicts() != 0
            || status.sparse
            || status
                .entries
                .iter()
                .any(|entry| entry.staged() && entry.path != path)
        {
            return Err(unavailable());
        }
        let (fingerprint, config) = repository.fingerprint_and_config_v9(256 * 1024)?;
        if config != self.config {
            return Err(unavailable());
        }
        Ok((status, fingerprint, config))
    }
    pub(crate) fn branch_matches_v9(&self, status: &RepoStatus) -> bool {
        self.isolated_branch
            .as_ref()
            .is_none_or(|branch| status.branch.as_ref() == Some(branch))
    }
    pub(crate) fn execute_save_v9(
        self: &Arc<Self>,
        request: &crate::scoped_actions_workflow_edit_v9::WorkflowSaveConfirmV9,
        provider: &GithubProvider,
        database: &Path,
    ) -> GithubResult<crate::scoped_actions_workflow_edit_v9::WorkflowSaveOutcomeV9> {
        use std::sync::TryLockError;
        let check = || request.check_original_v9(self);
        check()?;
        // The SAME common-dir map/mutex used by GitRepository::execute, not
        // a parallel writer lock. Waiting remains inside the captured clock.
        let locks = MUTATION_LOCKS.get_or_init(Mutex::default);
        let mut map = loop {
            check()?;
            match locks.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        };
        let lock = map
            .entry(self.origin.original_repository_v1().common_dir.clone())
            .or_default()
            .clone();
        drop(map);
        let guard = loop {
            check()?;
            match lock.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        };
        let witness = WorkflowMutationLockV9 {
            binding: self,
            _guard: &guard,
        };
        request.execute_locked_v9(provider, database, &witness)
    }
    pub(crate) fn execute_stage_v9(
        self: &Arc<Self>,
        request: &crate::scoped_actions_workflow_edit_v9::WorkflowStageConfirmV9,
        provider: &GithubProvider,
        database: &Path,
    ) -> GithubResult<crate::scoped_actions_workflow_edit_v9::WorkflowStageOutcomeV9> {
        use std::sync::TryLockError;
        let check = || request.check_original_v9(self);
        check()?;
        // Identical common-dir lock registry to Save and ordinary execute.
        // This closed method accepts no operation/closure from a caller.
        let locks = MUTATION_LOCKS.get_or_init(Mutex::default);
        let mut map = loop {
            check()?;
            match locks.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        };
        let lock = map
            .entry(self.origin.original_repository_v1().common_dir.clone())
            .or_default()
            .clone();
        drop(map);
        let guard = loop {
            check()?;
            match lock.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        };
        let witness = WorkflowMutationLockV9 {
            binding: self,
            _guard: &guard,
        };
        request.execute_locked_v9(provider, database, &witness)
    }
    /// Caller already performed genuine current Core/source SQL closure
    /// ordering. The mapping is merely the actual private name-only candidate;
    /// ONLY this installed-Git observer proves matching C/path bytes.
    pub(crate) fn execute_commit_v9(
        self: &Arc<Self>,
        request: &crate::scoped_actions_workflow_edit_v9::WorkflowCommitConfirmV9,
        provider: &GithubProvider,
        database: &Path,
    ) -> GithubResult<crate::scoped_actions_workflow_edit_v9::WorkflowCommitOutcomeV9> {
        use std::sync::TryLockError;
        let check = || request.check_original_v9(self);
        check()?;
        let locks = MUTATION_LOCKS.get_or_init(Mutex::default);
        let mut map = loop {
            check()?;
            match locks.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        };
        let lock = map
            .entry(self.origin.original_repository_v1().common_dir.clone())
            .or_default()
            .clone();
        drop(map);
        let guard = loop {
            check()?;
            match lock.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        };
        let witness = WorkflowMutationLockV9 {
            binding: self,
            _guard: &guard,
        };
        request.execute_locked_v9(provider, database, &witness)
    }
    pub(crate) fn execute_remote_v9(
        self: &Arc<Self>,
        request: &crate::scoped_actions_workflow_edit_v9::WorkflowRemoteConfirmV9,
        provider: &GithubProvider,
        database: &Path,
        drafts: &crate::drafts::DraftClient,
        fence: crate::drafts::DraftPublicationFenceV2,
    ) -> GithubResult<crate::actions_controller::WorkflowRemoteExecutionV9> {
        use std::sync::TryLockError;
        let check = || request.check_original_v9(self);
        check()?;
        let locks = MUTATION_LOCKS.get_or_init(Mutex::default);
        let mut map = loop {
            check()?;
            match locks.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        };
        let lock = map
            .entry(self.origin.original_repository_v1().common_dir.clone())
            .or_default()
            .clone();
        drop(map);
        let guard = loop {
            check()?;
            match lock.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        };
        let witness = WorkflowMutationLockV9 {
            binding: self,
            _guard: &guard,
        };
        request.start_v9(&witness)?;
        crate::actions_controller::execute_workflow_remote_v9(
            request, provider, database, drafts, fence, &witness,
        )
    }
    pub(crate) fn read(
        intent: Arc<WorkflowEditIntentV9>,
        mapping: Arc<RemoteMappingCandidateV1>,
        window: RemoteGitReadWindowV1,
        cancel: &CancellationToken,
    ) -> GitResult<Arc<Self>> {
        window.check(cancel).map_err(|_| unavailable())?;
        if mapping.mapping().scope != *intent.source().original().scope()
            || !mapping
                .mapping()
                .repository
                .eq_ignore_ascii_case(&intent.source().reference().last_name)
        {
            return Err(unavailable());
        }
        let directory = mapping.mapping().path();
        let context =
            WorkflowGitReadEnvironmentV9::capture_v9(&intent, &directory, &window, cancel)?;
        let repository = GitRepository::discover_with_remote_read_window_v1(
            &directory,
            cancel.clone(),
            window.with_workflow_environment_v9(context),
        )?
        .ok_or_else(unavailable)?;
        if repository.bare {
            return Err(unavailable());
        }
        let origin = OriginalLocalActorOriginV1::capture(&repository)?;
        let root_descriptor = file_v9::root_descriptor_v9(&origin)?;
        let status = repository.status()?;
        if status.operation.is_some()
            || status.has_history_overrides()
            || status.conflicts() != 0
            || status.sparse
        {
            return Err(unavailable());
        }
        let (observed, config) = repository.fingerprint_and_config_v9(256 * 1024)?;
        let path = RawPath(intent.definition().workflow.path.as_bytes().to_vec());
        let entry = repository.tree_entry(&intent.definition().commit_sha, &path)?;
        if entry.path != path
            || entry.kind != "blob"
            || !matches!(entry.mode.as_str(), "100644" | "100755")
        {
            return Err(unavailable());
        }
        let size = repository.object_sizes(std::slice::from_ref(&entry.id))?;
        if size.as_slice() != [intent.definition().yaml.len()] || size[0] > 256 * 1024 {
            return Err(unavailable());
        }
        let content = repository.object(&entry.id)?;
        if content.as_slice() != intent.definition().yaml.as_bytes() {
            return Err(unavailable());
        }
        // Drop the temporary blob before the second bounded config capture.
        drop(content);
        let (after, after_config) = repository.fingerprint_and_config_v9(256 * 1024)?;
        let after_status = repository.status()?;
        if observed != after || config != after_config || status != after_status {
            return Err(unavailable());
        }
        origin.recheck(cancel)?;
        repository.check_mutable_read_v2()?;
        Ok(Arc::new(Self {
            intent,
            mapping,
            origin,
            root_descriptor,
            parent: None,
            isolated_branch: None,
            config: after_config,
            mode: entry.mode,
            blob: entry.id,
        }))
    }
    pub(crate) fn intent(&self) -> &Arc<WorkflowEditIntentV9> {
        &self.intent
    }
    pub(crate) fn mode(&self) -> &str {
        &self.mode
    }
    pub(crate) fn blob(&self) -> &str {
        &self.blob
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.mapping.retained_bytes_v9()
            + 2 * std::mem::size_of::<usize>()
            + self.origin.retained_graph_memory_bytes_v2()
            + self.config.capacity()
            + self.mode.capacity()
            + self.blob.capacity()
            + self.isolated_branch.as_ref().map_or(0, String::capacity)
    }
    /// Worker only, after fresh original account/repository access. Source and
    /// the SAME raw mapping/file witness stay mandatory, including on failure.
    pub(crate) fn validate_local(
        &self,
        cache: &Cache,
        database: &Path,
        cancel: &CancellationToken,
    ) -> GitResult<()> {
        self.intent
            .source()
            .validate_local(database)
            .map_err(|_| unavailable())?;
        cache
            .validate_remote_mapping_candidate_v1(&self.mapping)
            .map_err(|_| unavailable())?;
        self.check_origin_v9(cancel)
    }
    /// The SAME current mapping/root checks after this actual Actions
    /// transaction's own claim/result. Only the existing typed source gate
    /// substitutes those exact durable aliases; all foreign guards remain.
    pub(crate) fn validate_owned_local_v9(
        &self,
        cache: &Cache,
        database: &Path,
        cancel: &CancellationToken,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        claim: &crate::actions_write_journal::ActionsClaimStampV3,
        result: Option<(
            crate::actions_write_journal::ActionsWriteState,
            &crate::actions_write_journal::ActionsReceipt,
        )>,
    ) -> GitResult<()> {
        self.intent
            .source()
            .validate_owned_publication_local_v4(database, journal, claim, result)
            .map_err(|_| unavailable())?;
        cache
            .validate_remote_mapping_candidate_v1(&self.mapping)
            .map_err(|_| unavailable())?;
        self.check_origin_v9(cancel)
    }
    pub(crate) fn root_label(&self) -> String {
        display_path_bytes(
            self.origin
                .original_repository_v1()
                .root
                .as_os_str()
                .as_bytes(),
        )
    }
    pub(crate) fn validate_after_own_branch_v9(
        &self,
        cache: &Cache,
        database: &Path,
        cancel: &CancellationToken,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        branch: &crate::scoped_actions_workflow_edit_v9::WorkflowPublishedBranchV9,
    ) -> GitResult<()> {
        self.intent
            .source()
            .validate_workflow_branch_transition_v9(database, journal, branch)
            .map_err(|_| unavailable())?;
        cache
            .validate_remote_mapping_candidate_v1(&self.mapping)
            .map_err(|_| unavailable())?;
        self.check_origin_v9(cancel)
    }
    pub(crate) fn root_display_bound_v9(&self) -> Option<usize> {
        self.origin
            .original_repository_v1()
            .root
            .as_os_str()
            .as_bytes()
            .len()
            .checked_mul(4)
    }
    /// Current regular bytes are separate from the matched saved blob C.
    /// Reading them never grants Save permission or changes the worktree.
    pub(crate) fn observe_working_file_v9(
        self: &Arc<Self>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GitResult<Arc<WorkflowWorkingFileV9>> {
        WorkflowWorkingFileV9::read(self.clone(), cancel, deadline)
    }
    #[cfg(test)]
    pub(crate) fn observe_working_file_after_read_v9(
        self: &Arc<Self>,
        cancel: &CancellationToken,
        deadline: Instant,
        after_actual_read: impl FnOnce(),
    ) -> GitResult<Arc<WorkflowWorkingFileV9>> {
        WorkflowWorkingFileV9::read_observed(self.clone(), cancel, deadline, after_actual_read)
    }
    fn read_command(
        self: &Arc<Self>,
        kind: ReadKindV9,
        phase: WorkflowEditPhaseV9,
        capture: &WorkflowPostCaptureV9,
        cancel: &CancellationToken,
    ) -> WorkflowGitCommandV9 {
        WorkflowGitCommandV9 {
            binding: self.clone(),
            kind,
            phase,
            cancel: cancel.clone(),
            deadline: capture.deadline(),
        }
    }
    pub(crate) fn post_resolve_head(
        self: &Arc<Self>,
        provider: &GithubProvider,
        phase: WorkflowEditPhaseV9,
        capture: &mut WorkflowPostCaptureV9,
        cancel: &CancellationToken,
    ) -> GithubResult<String> {
        let command = self.read_command(ReadKindV9::ResolveHead, phase, capture, cancel);
        let bytes = capture.git(provider, command, cancel)?.bytes()?;
        let head = text(trim_line(&bytes));
        if !original_actor_saved_sha(&head) {
            return Err(post_unavailable());
        }
        Ok(head)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkflowLocalStepV9 {
    Stage,
    Commit,
}
/// Only a real exact Save result can produce this bounded planner input.
/// It is neither an execution permit nor a caller-supplied Operation.
pub(crate) struct WorkflowPlanPurposeV9 {
    saved: Arc<crate::scoped_actions_workflow_edit_v9::WorkflowSavedLocalV9>,
    step: WorkflowLocalStepV9,
    cancel: CancellationToken,
    deadline: Instant,
}
impl WorkflowPlanPurposeV9 {
    pub(crate) fn from_saved_v9(
        saved: Arc<crate::scoped_actions_workflow_edit_v9::WorkflowSavedLocalV9>,
        step: WorkflowLocalStepV9,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GitResult<Self> {
        check_workflow_plan_clock_v9(cancel, deadline)?;
        saved.file().validate(cancel, deadline)?;
        Ok(Self {
            saved,
            step,
            cancel: cancel.clone(),
            deadline,
        })
    }
    pub(crate) fn validate(&self, repository: &GitRepository) -> GitResult<()> {
        check_workflow_plan_clock_v9(&self.cancel, self.deadline)?;
        let binding = self.saved.opened().binding();
        let original = binding.origin.original_repository_v1();
        if repository.root != original.root
            || repository.git_dir != original.git_dir
            || repository.common_dir != original.common_dir
            || !repository
                .actual_read_runner_v1()
                .same_instance(original.actual_read_runner_v1())
            || !repository
                .actual_cancellation_v1()
                .same_instance(&self.cancel)
            || repository
                .navigation_read_window
                .as_ref()
                .is_none_or(|window| window.deadline_v1() != self.deadline)
        {
            return Err(unavailable());
        }
        binding.check_origin_v9(&self.cancel)?;
        self.saved.file().validate(&self.cancel, self.deadline)
    }
    pub(crate) fn validate_status(&self, status: &RepoStatus) -> GitResult<()> {
        let intent = self.saved.opened().intent();
        let path = self.path();
        if !self.saved.opened().binding().branch_matches_v9(status)
            || status.head.as_deref() != Some(intent.definition().commit_sha.as_str())
            || status.operation.is_some()
            || status.conflicts() != 0
            || status.sparse
            || status.has_history_overrides()
            || status
                .entries
                .iter()
                .any(|entry| entry.staged() && entry.path != path)
            || (self.step == WorkflowLocalStepV9::Commit
                && !status
                    .entries
                    .iter()
                    .any(|entry| entry.staged() && entry.path == path))
            || (self.step == WorkflowLocalStepV9::Commit
                && status
                    .entries
                    .iter()
                    .any(|entry| entry.path == path && entry.worktree != '.'))
        {
            return Err(unavailable());
        }
        Ok(())
    }
    pub(crate) fn path(&self) -> RawPath {
        RawPath(
            self.saved
                .opened()
                .intent()
                .definition()
                .workflow
                .path
                .as_bytes()
                .to_vec(),
        )
    }
    pub(crate) fn command(&self) -> crate::local_actions::LocalCommand {
        match self.step {
            WorkflowLocalStepV9::Stage => crate::local_actions::LocalCommand::StageFile,
            WorkflowLocalStepV9::Commit => crate::local_actions::LocalCommand::Commit,
        }
    }
    pub(crate) fn operation(&self) -> Operation {
        match self.step {
            WorkflowLocalStepV9::Stage => Operation::Stage(vec![self.path()]),
            WorkflowLocalStepV9::Commit => Operation::Commit {
                message: self.saved.values().commit_message().into(),
                amend: false,
            },
        }
    }
    pub(crate) fn saved_content(&self) -> &str {
        self.saved.values().yaml()
    }
    pub(crate) fn original_base_v9(&self) -> &str {
        &self.saved.opened().intent().definition().commit_sha
    }
    pub(super) fn original_config_v9(&self) -> &[u8] {
        &self.saved.opened().binding().config
    }
    pub(crate) fn step(&self) -> WorkflowLocalStepV9 {
        self.step
    }
}
fn check_workflow_plan_clock_v9(cancel: &CancellationToken, deadline: Instant) -> GitResult<()> {
    if cancel.is_cancelled()
        || Instant::now() >= deadline
        || deadline.saturating_duration_since(Instant::now()) > Duration::from_secs(30)
    {
        return Err(unavailable());
    }
    Ok(())
}

/// Private construction and a real borrowed SAME mutex guard. Not a source,
/// current-state receipt, read-window promotion or generic writer permit.
pub(crate) struct WorkflowMutationLockV9<'a> {
    binding: &'a Arc<WorkflowLocalBindingV9>,
    _guard: &'a std::sync::MutexGuard<'a, ()>,
}
impl WorkflowMutationLockV9<'_> {
    pub(crate) fn owns(&self, binding: &Arc<WorkflowLocalBindingV9>) -> bool {
        Arc::ptr_eq(self.binding, binding)
    }
}

enum ReadKindV9 {
    ResolveHead,
    Status(u8),
    Fingerprint(u8),
    CommitSize(String),
    CommitObject(String),
    Snapshot {
        part: u8,
        base: String,
        head: String,
    },
}
/// Only actual binding/phase methods above may construct commands. In
/// particular no Write role is constructible from this READ-only type.
pub(crate) struct WorkflowGitCommandV9 {
    binding: Arc<WorkflowLocalBindingV9>,
    kind: ReadKindV9,
    phase: WorkflowEditPhaseV9,
    cancel: CancellationToken,
    deadline: Instant,
}
impl WorkflowGitCommandV9 {
    pub(crate) fn role(&self) -> WorkflowGitRoleV9 {
        match &self.kind {
            ReadKindV9::ResolveHead => WorkflowGitRoleV9::ResolveHead,
            ReadKindV9::Status(part) => WorkflowGitRoleV9::Status(*part),
            ReadKindV9::Fingerprint(part) => WorkflowGitRoleV9::Fingerprint(*part),
            ReadKindV9::CommitSize(_) => WorkflowGitRoleV9::CommitSize,
            ReadKindV9::CommitObject(_) => WorkflowGitRoleV9::CommitObject,
            ReadKindV9::Snapshot { part, .. } => WorkflowGitRoleV9::Snapshot(*part),
        }
    }
    pub(crate) fn validate_original(
        &self,
        runner: &CommandRunner,
        original: &Arc<ScopedActionsWorkflowV2>,
        phase: WorkflowEditPhaseV9,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        if !Arc::ptr_eq(self.binding.intent.original(), original)
            || phase != self.phase
            || deadline != self.deadline
            || !cancel.same_instance(&self.cancel)
            || cancel.is_cancelled()
            || Instant::now() >= deadline
            || !runner.same_instance(
                self.binding
                    .origin
                    .original_repository_v1()
                    .actual_read_runner_v1(),
            )
        {
            return Err(post_unavailable());
        }
        self.binding
            .check_origin_v9(cancel)
            .map_err(|_| post_unavailable())
    }
    pub(crate) fn execute_bounded(
        self,
        runner: &CommandRunner,
        cap: usize,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowGitOutputV9> {
        if !(1..=256 * 1024).contains(&cap)
            || deadline != self.deadline
            || !cancel.same_instance(&self.cancel)
            || cancel.is_cancelled()
            || Instant::now() >= deadline
        {
            return Err(post_unavailable());
        }
        self.binding
            .check_origin_v9(cancel)
            .map_err(|_| post_unavailable())?;
        let args: Vec<OsString> = match &self.kind {
            ReadKindV9::CommitSize(head) | ReadKindV9::CommitObject(head) => {
                if self.phase != WorkflowEditPhaseV9::Commit || !original_actor_saved_sha(head) {
                    return Err(post_unavailable());
                }
                let kind = if matches!(&self.kind, ReadKindV9::CommitSize(_)) {
                    "-s"
                } else {
                    "commit"
                };
                ["--no-replace-objects", "cat-file", kind, head]
                    .into_iter()
                    .map(OsString::from)
                    .collect()
            }
            ReadKindV9::Snapshot { part, base, head } => {
                if self.phase != WorkflowEditPhaseV9::Commit
                    || base != &self.binding.intent.definition().commit_sha
                    || !original_actor_saved_sha(base)
                    || !original_actor_saved_sha(head)
                    || *part > 3
                {
                    return Err(post_unavailable());
                }
                if *part < 2 {
                    let revision = format!("{}^{{commit}}", if *part == 0 { base } else { head });
                    [
                        "--no-replace-objects",
                        "rev-parse",
                        "--verify",
                        "--end-of-options",
                        &revision,
                    ]
                    .into_iter()
                    .map(OsString::from)
                    .collect()
                } else {
                    let mut args = vec![
                        OsString::from("--no-replace-objects"),
                        OsString::from("diff"),
                    ];
                    if *part == 3 {
                        args.extend([OsString::from("--name-status"), OsString::from("-z")]);
                    }
                    args.extend(
                        [
                            "--no-ext-diff",
                            "--no-textconv",
                            "--no-color",
                            "--find-renames",
                            base,
                            head,
                            "--",
                        ]
                        .into_iter()
                        .map(OsString::from),
                    );
                    args
                }
            }
            _ => {
                let fixed: &[&str] = match self.kind {
                    ReadKindV9::ResolveHead => &[
                        "--no-replace-objects",
                        "rev-parse",
                        "--verify",
                        "--end-of-options",
                        "HEAD^{commit}",
                    ],
                    ReadKindV9::Status(0) | ReadKindV9::Fingerprint(0) => &[
                        "status",
                        "--porcelain=v2",
                        "-z",
                        "--branch",
                        "--untracked-files=all",
                    ],
                    ReadKindV9::Status(1) => &["rev-parse", "--is-shallow-repository"],
                    ReadKindV9::Status(2) => &[
                        "config",
                        "--bool",
                        "--default=false",
                        "--get",
                        "core.sparseCheckout",
                    ],
                    ReadKindV9::Status(3) => &[
                        "for-each-ref",
                        "--format=%(refname)%00%(objectname)%00%(upstream)%00%(HEAD)%00",
                    ],
                    ReadKindV9::Fingerprint(1) => &["ls-files", "--stage", "-z"],
                    ReadKindV9::Fingerprint(2) => &[
                        "for-each-ref",
                        "--format=%(refname)%00%(objectname)%00%(upstream)%00",
                    ],
                    ReadKindV9::Fingerprint(3) => &["config", "--null", "--list"],
                    ReadKindV9::Fingerprint(4) => {
                        &["diff", "--no-ext-diff", "--no-textconv", "--binary"]
                    }
                    _ => return Err(post_unavailable()),
                };
                fixed.iter().map(|arg| OsString::from(*arg)).collect()
            }
        };
        let mut spec = git_command_spec_v9(
            &self.binding.origin.original_repository_v1().root,
            args,
            None,
            false,
            &GitReadPolicyV1 {
                timeout: deadline.saturating_duration_since(Instant::now()),
                output_limit: cap,
                window: None,
                runner: Some(runner),
            },
            Some(deadline),
        )
        .fresh_read();
        spec.original_read_cancellation = Some(self.cancel.clone());
        spec.workflow_environment_v9 = Some(
            WorkflowGitEnvironmentV9::capture_v9(
                &self.binding.intent,
                runner,
                cancel,
                deadline,
                &spec,
            )
            .map_err(|_| post_unavailable())?,
        );
        let output =
            git_command_output_v9(runner.run(spec, cancel)).map_err(|_| post_unavailable())?;
        self.binding
            .check_origin_v9(cancel)
            .map_err(|_| post_unavailable())?;
        if cancel.is_cancelled() || Instant::now() >= deadline || !output.success {
            return Err(post_unavailable());
        }
        Ok(WorkflowGitOutputV9 { output })
    }
}
pub(crate) struct WorkflowGitOutputV9 {
    output: GitOutput,
}
impl WorkflowGitOutputV9 {
    fn bytes(self) -> GithubResult<Vec<u8>> {
        self.output
            .success
            .then_some(self.output.stdout)
            .ok_or_else(post_unavailable)
    }
}
