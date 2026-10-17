//! Fixed Stage command, built before final UID and released only by the
//! actual explicit-once Confirm/ClosedCore/borrowed SAME mutation lock.
use super::*;
use crate::scoped_actions_workflow_edit_v9::{
    WorkflowCommitConfirmV9, WorkflowCommitReleaseV9, WorkflowLocalPlanPreviewV9,
    WorkflowStageConfirmV9, WorkflowStageReleaseV9,
};

pub(crate) struct WorkflowPreparedStageV9 {
    binding: Arc<WorkflowLocalBindingV9>,
    preview: Arc<WorkflowLocalPlanPreviewV9>,
    cancel: CancellationToken,
    deadline: Instant,
    phase: WorkflowEditPhaseV9,
    spec: CommandSpec,
}
impl WorkflowPreparedStageV9 {
    pub(crate) fn prepare(
        binding: &Arc<WorkflowLocalBindingV9>,
        request: &WorkflowStageConfirmV9,
        repository: &GitRepository,
        status: &RepoStatus,
        effective: Instant,
    ) -> GithubResult<Self> {
        request.check_original_v9(binding)?;
        if effective > request.deadline() {
            return Err(post_unavailable());
        }
        let preview = request.preview();
        Self::prepare_exact(
            binding,
            preview,
            repository,
            status,
            request.cancellation(),
            effective,
            WorkflowLocalStepV9::Stage,
        )
    }
    fn prepare_exact(
        binding: &Arc<WorkflowLocalBindingV9>,
        preview: &Arc<WorkflowLocalPlanPreviewV9>,
        repository: &GitRepository,
        status: &RepoStatus,
        cancel: &CancellationToken,
        deadline: Instant,
        step: WorkflowLocalStepV9,
    ) -> GithubResult<Self> {
        let purpose =
            WorkflowPlanPurposeV9::from_saved_v9(preview.saved().clone(), step, cancel, deadline)
                .map_err(|_| post_unavailable())?;
        purpose
            .validate(repository)
            .map_err(|_| post_unavailable())?;
        purpose
            .validate_status(status)
            .map_err(|_| post_unavailable())?;
        let operation_matches = match (&preview.plan().mutation.operation, step) {
            (Operation::Stage(paths), WorkflowLocalStepV9::Stage) => {
                paths.as_slice() == [purpose.path()]
            }
            (
                Operation::Commit {
                    message,
                    amend: false,
                },
                WorkflowLocalStepV9::Commit,
            ) => message == preview.saved().values().commit_message(),
            _ => false,
        };
        if preview.step() != step || !operation_matches {
            return Err(post_unavailable());
        }
        // Borrowed size gate before args/spec duplicate a raw path or root.
        // Stage has no stdin; fixed argv/env/header overhead is below64KiB.
        let spec_bound = binding
            .origin
            .original_repository_v1()
            .root
            .as_os_str()
            .len()
            .checked_add(binding.intent.definition().workflow.path.len())
            .and_then(|n| n.checked_add(preview.saved().values().commit_message().len()))
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| n.checked_add(64 * 1024))
            .and_then(|n| n.checked_add(preview.retained_bytes()))
            .ok_or_else(post_unavailable)?;
        if spec_bound > 8 * 1024 * 1024 {
            return Err(post_unavailable());
        }
        // SAME operation planner, fixed installed-git argv and child policy.
        // No generic execute is permitted while the navigation window exists.
        let args = repository
            .operation_args(&preview.plan().mutation.operation, status)
            .map_err(|_| post_unavailable())?;
        let input = match &preview.plan().mutation.operation {
            Operation::Commit {
                message,
                amend: false,
            } => Some(message.as_bytes()),
            Operation::Stage(_) => None,
            _ => return Err(post_unavailable()),
        };
        let mut spec = git_command_spec_v9(
            &repository.root,
            args,
            input,
            true,
            &GitReadPolicyV1 {
                timeout: deadline.saturating_duration_since(Instant::now()),
                output_limit: 256 * 1024,
                window: None,
                runner: Some(repository.actual_read_runner_v1()),
            },
            Some(deadline),
        );
        spec.workflow_environment_v9 = Some(
            WorkflowGitEnvironmentV9::capture_v9(
                &binding.intent,
                repository.actual_read_runner_v1(),
                cancel,
                deadline,
                &spec,
            )
            .map_err(|_| post_unavailable())?,
        );
        Ok(Self {
            binding: binding.clone(),
            preview: preview.clone(),
            cancel: cancel.clone(),
            deadline,
            phase: match step {
                WorkflowLocalStepV9::Stage => WorkflowEditPhaseV9::Stage,
                WorkflowLocalStepV9::Commit => WorkflowEditPhaseV9::Commit,
            },
            spec,
        })
    }
    pub(crate) fn release<'a>(
        self,
        release: &'a WorkflowStageReleaseV9<'a>,
    ) -> GithubResult<WorkflowGitWriteCommandV9<'a>> {
        release.check(&self.binding, &self.preview)?;
        if self.deadline != release.deadline() {
            return Err(post_unavailable());
        }
        Ok(WorkflowGitWriteCommandV9 {
            prepared: self,
            release: ReleaseV9::Stage(release),
        })
    }
}
pub(crate) struct WorkflowPreparedCommitV9(WorkflowPreparedStageV9);
impl WorkflowPreparedCommitV9 {
    pub(crate) fn prepare(
        binding: &Arc<WorkflowLocalBindingV9>,
        request: &WorkflowCommitConfirmV9,
        repository: &GitRepository,
        status: &RepoStatus,
        effective: Instant,
    ) -> GithubResult<Self> {
        request.check_original_v9(binding)?;
        if effective > request.deadline() {
            return Err(post_unavailable());
        }
        WorkflowPreparedStageV9::prepare_exact(
            binding,
            request.preview(),
            repository,
            status,
            request.cancellation(),
            effective,
            WorkflowLocalStepV9::Commit,
        )
        .map(Self)
    }
    pub(crate) fn release<'a>(
        self,
        release: &'a WorkflowCommitReleaseV9<'a>,
    ) -> GithubResult<WorkflowGitWriteCommandV9<'a>> {
        release.check(&self.0.binding, &self.0.preview)?;
        if self.0.deadline != release.deadline() {
            return Err(post_unavailable());
        }
        Ok(WorkflowGitWriteCommandV9 {
            prepared: self.0,
            release: ReleaseV9::Commit(release),
        })
    }
}
enum ReleaseV9<'a> {
    Stage(&'a WorkflowStageReleaseV9<'a>),
    Commit(&'a WorkflowCommitReleaseV9<'a>),
}
impl ReleaseV9<'_> {
    fn check(
        &self,
        binding: &Arc<WorkflowLocalBindingV9>,
        preview: &Arc<WorkflowLocalPlanPreviewV9>,
    ) -> GithubResult<()> {
        match self {
            Self::Stage(release) => release.check(binding, preview),
            Self::Commit(release) => release.check(binding, preview),
        }
    }
}

/// No public constructor, Clone, serde, raw args/spec or runner getter.
pub(crate) struct WorkflowGitWriteCommandV9<'a> {
    prepared: WorkflowPreparedStageV9,
    release: ReleaseV9<'a>,
}
impl WorkflowGitWriteCommandV9<'_> {
    pub(crate) fn validate_original(
        &self,
        runner: &CommandRunner,
        original: &Arc<ScopedActionsWorkflowV2>,
        phase: WorkflowEditPhaseV9,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        let prepared = &self.prepared;
        if phase != prepared.phase
            || deadline != prepared.deadline
            || !cancel.same_instance(&prepared.cancel)
            || !Arc::ptr_eq(prepared.binding.intent.original(), original)
            || !runner.same_instance(
                prepared
                    .binding
                    .origin
                    .original_repository_v1()
                    .actual_read_runner_v1(),
            )
        {
            return Err(post_unavailable());
        }
        self.release.check(&prepared.binding, &prepared.preview)
    }
    pub(crate) fn execute_bounded(
        mut self,
        runner: &CommandRunner,
        cap: usize,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.validate_original(
            runner,
            self.prepared.binding.intent.original(),
            self.prepared.phase,
            cancel,
            deadline,
        )?;
        if !(1..=256 * 1024).contains(&cap) {
            return Err(post_unavailable());
        }
        self.prepared.spec.output_limit = self.prepared.spec.output_limit.min(cap);
        self.prepared.spec.timeout = self
            .prepared
            .spec
            .timeout
            .min(deadline.saturating_duration_since(Instant::now()));
        // Original absolute deadline survives serialization/setup/runner lock.
        // This is an actual Write; no navigation READ window is attached.
        let result = git_command_output_v9(runner.run(self.prepared.spec, cancel))
            .map_err(|_| post_unavailable())?;
        if !result.success || cancel.is_cancelled() || Instant::now() >= deadline {
            return Err(post_unavailable());
        }
        self.prepared
            .binding
            .check_origin_v9(cancel)
            .map_err(|_| post_unavailable())
    }
}
