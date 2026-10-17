//! Actual committed W9 receipt plus original installed-Git remote facts.
//! This read observation cannot release a push, create a remote, or recover a
//! source/permission from a persisted path, name or commit ID.
use super::*;
use crate::{
    providers::gh::WorkflowRemoteRepositoryV9,
    scoped_actions_workflow_edit_v9::WorkflowCommittedLocalV9,
};

pub(crate) struct WorkflowRemoteGitObservationV9 {
    committed: Arc<WorkflowCommittedLocalV9>,
    remote: Arc<WorkflowRemoteRepositoryV9>,
    cancel: CancellationToken,
    deadline: Instant,
    phase: WorkflowEditPhaseV9,
    remote_name: String,
    url: String,
    branch: String,
    raw_config: Vec<u8>,
    fingerprint: WorkspaceFingerprint,
}
impl fmt::Debug for WorkflowRemoteGitObservationV9 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkflowRemoteGitObservationV9(<actual committed/source/remote observation; no write permit>)")
    }
}
fn bad() -> GitError {
    GitError("Original committed workflow/remote/config/ref changed or unavailable; no URL rewrite or fallback".into())
}
fn check(cancel: &CancellationToken, deadline: Instant) -> GitResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(bad())
    } else {
        Ok(())
    }
}
fn configured_remote(config: &[u8], name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && !name.starts_with('-')
        && !name.chars().any(|c| c.is_control() || c.is_whitespace())
        && config
            .split(|b| *b == 0)
            .filter_map(|row| row.split(|b| *b == b'\n').next())
            .any(|key| key == format!("remote.{name}.url").as_bytes())
}
impl WorkflowRemoteGitObservationV9 {
    pub(crate) fn observe(
        committed: &Arc<WorkflowCommittedLocalV9>,
        repository: &GitRepository,
        remote: &Arc<WorkflowRemoteRepositoryV9>,
        remote_name: &str,
        phase: WorkflowEditPhaseV9,
    ) -> GitResult<Arc<Self>> {
        if !matches!(
            phase,
            WorkflowEditPhaseV9::PublishBranch | WorkflowEditPhaseV9::CreatePr
        ) {
            return Err(bad());
        }
        let saved = committed.staged().saved();
        let binding = saved.opened().binding();
        let window = repository.navigation_read_window.as_ref().ok_or_else(bad)?;
        let cancel = repository.actual_cancellation_v1();
        let deadline = window.deadline_v1();
        check(cancel, deadline)?;
        window.check(cancel).map_err(|_| bad())?;
        if !remote.matches_git_window_v9(binding.intent(), window, cancel)
            || !repository.actual_read_runner_v1().same_instance(
                binding
                    .origin
                    .original_repository_v1()
                    .actual_read_runner_v1(),
            )
            || repository.root != binding.origin.original_repository_v1().root
            || repository.git_dir != binding.origin.original_repository_v1().git_dir
            || repository.common_dir != binding.origin.original_repository_v1().common_dir
            || repository.bare
        {
            return Err(bad());
        }
        binding.check_origin_v9(cancel)?;
        saved.file().validate(cancel, deadline)?;
        // Only these genuine fresh repository facts bind transport operands;
        // the ordinary read context cannot replay arbitrary ls-remote URLs.
        let environment_repository = WorkflowGitReadEnvironmentV9::remote_repository_v9(
            committed,
            remote,
            remote_name,
            repository,
        )?;
        let repository = &environment_repository;
        let status = repository.status()?;
        let path = RawPath(
            binding
                .intent
                .definition()
                .workflow
                .path
                .as_bytes()
                .to_vec(),
        );
        if status != *committed.status()
            || status.head.as_deref() != Some(committed.commit().head())
            || !binding.branch_matches_v9(&status)
            || status.operation.is_some()
            || status.has_history_overrides()
            || status.conflicts() != 0
            || status.sparse
            || status
                .entries
                .iter()
                .any(|entry| entry.staged() || entry.path == path)
        {
            return Err(bad());
        }
        let (fingerprint, raw_config) = repository.fingerprint_and_config_v9(256 * 1024)?;
        if fingerprint != committed.fingerprint()
            || raw_config != binding.config
            || !configured_remote(&raw_config, remote_name)
        {
            return Err(bad());
        }
        let bytes = repository.bytes_with_capture_limit_v2(
            ["remote", "get-url", "--push", "--all", remote_name],
            None,
            16 * 1024,
        )?;
        let raw = std::str::from_utf8(&bytes).map_err(|_| bad())?;
        let url = raw.strip_suffix('\n').unwrap_or(raw);
        if url.is_empty()
            || url.contains(['\n', '\r'])
            || url.chars().any(char::is_control)
            || url != remote.clone_url() && url != remote.ssh_url()
        {
            return Err(bad());
        }
        let branch = saved.values().branch();
        if phase == WorkflowEditPhaseV9::PublishBranch {
            repository.bytes_with_capture_limit_v2(
                ["check-ref-format", "--branch", branch],
                None,
                4 * 1024,
            )?;
        }
        let head = repository.bytes_with_capture_limit_v2(
            ["rev-parse", "--verify", "HEAD"],
            None,
            4 * 1024,
        )?;
        if trim_line(&head) != committed.commit().head().as_bytes() {
            return Err(bad());
        }
        if phase == WorkflowEditPhaseV9::PublishBranch {
            let exact = format!("refs/heads/{branch}");
            let refs = repository.bytes_with_capture_limit_v2(
                ["ls-remote", "--refs", "--", url, &exact],
                None,
                8 * 1024,
            )?;
            // Actual exit0 + zero bytes only. Whitespace/malformed/prefix
            // results are NOT empty evidence; no API404 overrides Git facts.
            if !refs.is_empty() {
                return Err(bad());
            }
        }
        saved.file().validate(cancel, deadline)?;
        binding.check_origin_v9(cancel)?;
        check(cancel, deadline)?;
        Ok(Arc::new(Self {
            committed: committed.clone(),
            remote: remote.clone(),
            cancel: cancel.clone(),
            deadline,
            phase,
            remote_name: remote_name.into(),
            url: url.into(),
            branch: branch.into(),
            raw_config,
            fingerprint,
        }))
    }
    pub(crate) fn committed(&self) -> &Arc<WorkflowCommittedLocalV9> {
        &self.committed
    }
    pub(crate) fn remote(&self) -> &Arc<WorkflowRemoteRepositoryV9> {
        &self.remote
    }
    pub(crate) fn remote_name(&self) -> &str {
        &self.remote_name
    }
    pub(crate) fn url(&self) -> &str {
        &self.url
    }
    pub(crate) fn branch(&self) -> &str {
        &self.branch
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn phase(&self) -> WorkflowEditPhaseV9 {
        self.phase
    }
    pub(crate) fn same_observation(&self, new: &Self) -> bool {
        Arc::ptr_eq(&self.committed, &new.committed)
            && self.remote.id() == new.remote.id()
            && self.remote.node() == new.remote.node()
            && self.remote.name() == new.remote.name()
            && self.remote_name == new.remote_name
            && self.url == new.url
            && self.branch == new.branch
            && self.raw_config == new.raw_config
            && self.fingerprint == new.fingerprint
            && self.phase == new.phase
    }
    pub(crate) fn owned_bytes_v9(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.remote_name.capacity()
            + self.url.capacity()
            + self.branch.capacity()
            + self.raw_config.capacity()
    }
}

/// Fixed argv are allocated while the measured phase is still live, before its
/// closing principal. No caller supplied args, URL, spec or runner escapes.
pub(crate) struct WorkflowPreparedPushV9 {
    observation: Arc<WorkflowRemoteGitObservationV9>,
    write: CommandSpec,
    read: CommandSpec,
}
impl WorkflowPreparedPushV9 {
    pub(crate) fn prepare_v9(
        observation: &Arc<WorkflowRemoteGitObservationV9>,
    ) -> GithubResult<Self> {
        let current = observation.as_ref();
        check(&current.cancel, current.deadline).map_err(|_| post_unavailable())?;
        if current.phase != WorkflowEditPhaseV9::PublishBranch {
            return Err(post_unavailable());
        }
        let binding = current.committed.staged().saved().opened().binding();
        binding
            .check_origin_v9(&current.cancel)
            .map_err(|_| post_unavailable())?;
        let repo = binding.origin.original_repository_v1();
        let exact_ref = format!("refs/heads/{}", current.branch);
        let lease = format!("--force-with-lease={exact_ref}:");
        let update = format!("{}:{exact_ref}", current.committed.commit().head());
        let policy = GitReadPolicyV1 {
            timeout: current.deadline.saturating_duration_since(Instant::now()),
            output_limit: 256 * 1024,
            window: None,
            runner: Some(repo.actual_read_runner_v1()),
        };
        let mut write = git_command_spec_v9(
            &repo.root,
            vec![
                "push".into(),
                "--porcelain".into(),
                lease,
                "--".into(),
                current.url.clone(),
                update,
            ],
            None,
            true,
            &policy,
            Some(current.deadline),
        );
        let mut read = git_command_spec_v9(
            &repo.root,
            vec![
                "ls-remote".into(),
                "--refs".into(),
                "--".into(),
                current.url.clone(),
                exact_ref,
            ],
            None,
            false,
            &policy,
            Some(current.deadline),
        );
        read = read.fresh_read();
        read.output_limit = 8 * 1024;
        for spec in [&mut write, &mut read] {
            spec.workflow_environment_v9 = Some(
                WorkflowGitEnvironmentV9::capture_v9(
                    &binding.intent,
                    repo.actual_read_runner_v1(),
                    &current.cancel,
                    current.deadline,
                    spec,
                )
                .map_err(|_| post_unavailable())?,
            );
        }
        Ok(Self {
            observation: observation.clone(),
            write,
            read,
        })
    }
    pub(crate) fn retained_bytes_v9(&self) -> usize {
        fn heap(spec: &CommandSpec) -> usize {
            spec.args.capacity() * std::mem::size_of::<std::ffi::OsString>()
                + spec.args.iter().map(|a| a.capacity()).sum::<usize>()
                + spec
                    .env
                    .iter()
                    .map(|(k, v)| k.capacity() + v.capacity() + 128)
                    .sum::<usize>()
                + spec.stdin.capacity()
                + spec.sensitive_values.capacity() * std::mem::size_of::<String>()
                + spec
                    .sensitive_values
                    .iter()
                    .map(String::capacity)
                    .sum::<usize>()
                + spec.cwd.as_ref().map_or(0, |p| p.capacity())
                + match &spec.kind {
                    crate::providers::runner::CommandKind::Write { resource } => {
                        resource.capacity()
                    }
                    _ => 0,
                }
        }
        std::mem::size_of::<Self>() + heap(&self.write) + heap(&self.read)
    }
    pub(crate) fn release_v9<'a>(
        self,
        release: &'a crate::scoped_actions_workflow_edit_v9::WorkflowRemoteReleaseV9<'a>,
    ) -> GithubResult<(
        WorkflowRemoteGitCommandV9<'a>,
        WorkflowRemoteGitCommandV9<'a>,
    )> {
        release.check_v9(&self.observation)?;
        let make = |spec, role| WorkflowRemoteGitCommandV9 {
            observation: self.observation.clone(),
            release,
            spec,
            role,
        };
        Ok((
            make(self.write, WorkflowGitRoleV9::Write),
            make(self.read, WorkflowGitRoleV9::ExactPushedRef),
        ))
    }
}
pub(crate) struct WorkflowRemoteGitCommandV9<'a> {
    observation: Arc<WorkflowRemoteGitObservationV9>,
    release: &'a crate::scoped_actions_workflow_edit_v9::WorkflowRemoteReleaseV9<'a>,
    spec: CommandSpec,
    role: WorkflowGitRoleV9,
}
impl WorkflowRemoteGitCommandV9<'_> {
    pub(crate) fn role(&self) -> WorkflowGitRoleV9 {
        self.role
    }
    pub(crate) fn validate_original(
        &self,
        runner: &CommandRunner,
        original: &Arc<ScopedActionsWorkflowV2>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        let observation = &self.observation;
        let binding = observation.committed.staged().saved().opened().binding();
        if observation.phase != WorkflowEditPhaseV9::PublishBranch
            || deadline != observation.deadline
            || deadline != self.release.deadline()
            || !cancel.same_instance(&observation.cancel)
            || !runner.same_instance(
                binding
                    .origin
                    .original_repository_v1()
                    .actual_read_runner_v1(),
            )
            || !Arc::ptr_eq(binding.intent.original(), original)
            || self.spec.absolute_deadline_v1 != Some(deadline)
        {
            return Err(post_unavailable());
        }
        self.release.check_v9(observation)
    }
    pub(crate) fn execute_bounded(
        mut self,
        runner: &CommandRunner,
        cap: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.validate_original(
            runner,
            self.observation
                .committed
                .staged()
                .saved()
                .opened()
                .intent()
                .original(),
            cancel,
            self.release.deadline(),
        )?;
        if !(1..=256 * 1024).contains(&cap) {
            return Err(post_unavailable());
        }
        self.spec.output_limit = self.spec.output_limit.min(cap);
        self.spec.timeout = self.spec.timeout.min(
            self.release
                .deadline()
                .saturating_duration_since(Instant::now()),
        );
        let result =
            git_command_output_v9(runner.run(self.spec, cancel)).map_err(|_| post_unavailable())?;
        if !result.success {
            return Err(post_unavailable());
        }
        if self.role == WorkflowGitRoleV9::ExactPushedRef {
            let expected = format!(
                "{}\trefs/heads/{}\n",
                self.observation.committed.commit().head(),
                self.observation.branch
            );
            if result.stdout != expected.as_bytes() {
                return Err(post_unavailable());
            }
        }
        self.release.check_v9(&self.observation)
    }
}
