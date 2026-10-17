//! Restriction-only startup replay, minted by the actual fixed W9 factories.
//! No arbitrary environment, command, runner or source constructor escapes.
use super::*;
use crate::providers::runner::{CommandKind, WorkflowProcessEnvironmentV9};
use sha2::{Digest, Sha256};

pub(crate) struct WorkflowGitEnvironmentV9 {
    environment: Arc<WorkflowProcessEnvironmentV9>,
    original: Arc<ScopedActionsWorkflowV2>,
    runner: CommandRunner,
    cancel: CancellationToken,
    deadline: Instant,
    identity: [u8; 32],
    max_output: usize,
    max_timeout: Duration,
    pool: Option<Arc<crate::providers::runner::ScopedReadBudgetV1>>,
    read_cancel: Option<CancellationToken>,
    mask: Option<Arc<WorkflowWorktreeMaskV9>>,
}
impl fmt::Debug for WorkflowGitEnvironmentV9 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkflowGitEnvironmentV9(<fixed original child; sealed environment redacted>)")
    }
}
fn hash_spec(
    spec: &CommandSpec,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GitResult<[u8; 32]> {
    let mut hash = Sha256::new();
    let mut bytes = |value: &[u8]| -> GitResult<()> {
        hash.update((value.len() as u64).to_le_bytes());
        for chunk in value.chunks(256) {
            check_workflow_plan_clock_v9(cancel, deadline)?;
            hash.update(chunk);
        }
        Ok(())
    };
    bytes(spec.program.as_bytes())?;
    bytes(&(spec.args.len() as u64).to_le_bytes())?;
    for arg in &spec.args {
        bytes(arg.as_bytes())?;
    }
    bytes(&[u8::from(spec.cwd.is_some())])?;
    if let Some(cwd) = &spec.cwd {
        bytes(cwd.as_os_str().as_bytes())?;
    }
    bytes(&(spec.env.len() as u64).to_le_bytes())?;
    for (key, value) in &spec.env {
        bytes(key.as_bytes())?;
        bytes(value.as_bytes())?;
    }
    bytes(&[u8::from(spec.clear_env), u8::from(spec.coalesce_read)])?;
    bytes(&spec.stdin)?;
    match &spec.kind {
        CommandKind::Read => bytes(b"Read")?,
        CommandKind::Write { resource } => {
            bytes(b"Write")?;
            bytes(resource.as_bytes())?;
        }
    }
    bytes(&(spec.sensitive_values.len() as u64).to_le_bytes())?;
    for value in &spec.sensitive_values {
        bytes(value.as_bytes())?;
    }
    Ok(hash.finalize().into())
}
fn same_optional_arc<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}
fn same_optional_cancel(a: &Option<CancellationToken>, b: &Option<CancellationToken>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_instance(b),
        _ => false,
    }
}
impl WorkflowGitEnvironmentV9 {
    /// Called ONLY after the producing closed W9 command validated its actual
    /// source/role/root/operation. The phase charged this bound before copies.
    pub(super) fn capture_v9(
        intent: &Arc<WorkflowEditIntentV9>,
        runner: &CommandRunner,
        cancel: &CancellationToken,
        deadline: Instant,
        spec: &CommandSpec,
    ) -> GitResult<Arc<Self>> {
        check_workflow_plan_clock_v9(cancel, deadline)?;
        let environment = WorkflowProcessEnvironmentV9::current_v9().ok_or_else(unavailable)?;
        if spec.program != "git"
            || spec.clear_env
            || spec.workflow_environment_v9.is_some()
            || !spec.sensitive_values.is_empty()
            || spec.output_limit == 0
            || spec.output_limit > 256 * 1024
            || spec.timeout.is_zero()
            || (spec.kind == CommandKind::Read && spec.coalesce_read)
            || spec.absolute_deadline_v1 != Some(deadline)
            || environment
                .child_bound_v9(&spec.env)
                .is_none_or(|n| n > environment.fixed_git_child_bound_v9().unwrap_or(0))
        {
            return Err(unavailable());
        }
        let identity = hash_spec(spec, cancel, deadline)?;
        Ok(Arc::new(Self {
            environment,
            original: intent.original().clone(),
            runner: runner.clone(),
            cancel: cancel.clone(),
            deadline,
            identity,
            max_output: spec.output_limit,
            max_timeout: spec.timeout,
            pool: spec.scoped_read_budget.clone(),
            read_cancel: spec.original_read_cancellation.clone(),
            mask: spec.workflow_worktree_mask_v9.clone(),
        }))
    }
    pub(crate) fn validate_spec_v9(
        &self,
        spec: &CommandSpec,
        cancel: &CancellationToken,
    ) -> GitResult<()> {
        check_workflow_plan_clock_v9(cancel, self.deadline)?;
        if !cancel.same_instance(&self.cancel)
            || !spec
                .workflow_environment_v9
                .as_ref()
                .is_some_and(|token| std::ptr::eq(self, token.as_ref()))
            || spec.absolute_deadline_v1 != Some(self.deadline)
            || spec.output_limit == 0
            || spec.output_limit > self.max_output
            || spec.timeout.is_zero()
            || spec.timeout > self.max_timeout
            || !same_optional_arc(&spec.scoped_read_budget, &self.pool)
            || !same_optional_arc(&spec.workflow_worktree_mask_v9, &self.mask)
            || !same_optional_cancel(&spec.original_read_cancellation, &self.read_cancel)
            || hash_spec(spec, cancel, self.deadline)? != self.identity
        {
            return Err(unavailable());
        }
        Ok(())
    }
    pub(crate) fn configure_command_v9(
        &self,
        spec: &CommandSpec,
        cancel: &CancellationToken,
        command: &mut std::process::Command,
    ) -> GitResult<()> {
        self.validate_spec_v9(spec, cancel)?;
        self.environment
            .configure_git_command_v9(self, spec, cancel, command)
    }
    pub(crate) fn same_environment_v9(&self, snapshot: &WorkflowProcessEnvironmentV9) -> bool {
        std::ptr::eq(self.environment.as_ref(), snapshot)
    }
    pub(crate) fn validate_runner_v9(&self, runner: &CommandRunner) -> GitResult<()> {
        // Actual producing runner remains mandatory even though the token
        // cannot release a command, acquire a lock or grant a write.
        if !self.runner.same_instance(runner) || self.original.repository().id() == 0 {
            return Err(unavailable());
        }
        check_workflow_plan_clock_v9(&self.cancel, self.deadline)
    }
    #[cfg(test)]
    pub(crate) fn test_matches_command_environment_v9(
        &self,
        spec: &CommandSpec,
        command: &std::process::Command,
    ) -> bool {
        self.environment
            .test_matches_git_environment_v9(spec, command)
    }
}

/// A measured original W9 window may mint a DISTINCT token per fixed read.
/// Construction stays inside the actual initial/binding factories below.
pub(crate) struct WorkflowGitReadEnvironmentV9 {
    intent: Arc<WorkflowEditIntentV9>,
    directory: PathBuf,
    runner: CommandRunner,
    cancel: CancellationToken,
    deadline: Instant,
    window: RemoteGitReadWindowV1,
    observed_root: Mutex<Option<PathBuf>>,
    remote: Option<(
        Arc<crate::providers::gh::WorkflowRemoteRepositoryV9>,
        Arc<crate::scoped_actions_workflow_edit_v9::WorkflowCommittedLocalV9>,
        String,
    )>,
}
impl WorkflowGitReadEnvironmentV9 {
    pub(super) fn capture_v9(
        intent: &Arc<WorkflowEditIntentV9>,
        directory: &Path,
        window: &RemoteGitReadWindowV1,
        cancel: &CancellationToken,
    ) -> GitResult<Arc<Self>> {
        window.check(cancel).map_err(|_| unavailable())?;
        check_workflow_plan_clock_v9(cancel, window.deadline_v1())?;
        if !directory.is_absolute() || directory.as_os_str().len() > 4096 {
            return Err(unavailable());
        }
        WorkflowProcessEnvironmentV9::current_v9()
            .and_then(|v| v.fixed_git_child_bound_v9())
            .ok_or_else(unavailable)?;
        Ok(Arc::new(Self {
            intent: intent.clone(),
            directory: directory.into(),
            runner: window.actual_runner_v1().clone(),
            cancel: cancel.clone(),
            deadline: window.deadline_v1(),
            window: window.clone(),
            observed_root: Mutex::new(None),
            remote: None,
        }))
    }
    pub(super) fn remote_repository_v9(
        committed: &Arc<crate::scoped_actions_workflow_edit_v9::WorkflowCommittedLocalV9>,
        remote: &Arc<crate::providers::gh::WorkflowRemoteRepositoryV9>,
        name: &str,
        repository: &GitRepository,
    ) -> GitResult<GitRepository> {
        let binding = committed.staged().saved().opened().binding();
        let window = repository
            .navigation_read_window
            .as_ref()
            .ok_or_else(unavailable)?;
        let cancel = repository.actual_cancellation_v1();
        if name.len() > 256
            || !remote.matches_git_window_v9(&binding.intent, window, cancel)
            || repository.root != binding.origin.original_repository_v1().root
        {
            return Err(unavailable());
        }
        let mut context = Self::capture_v9(&binding.intent, &repository.root, window, cancel)?;
        context.observe_discovered_root_v9(&repository.root, cancel)?;
        Arc::get_mut(&mut context).ok_or_else(unavailable)?.remote =
            Some((remote.clone(), committed.clone(), name.into()));
        let mut repository = repository.clone();
        repository.navigation_read_window =
            Some(window.clone().with_workflow_environment_v9(context));
        Ok(repository)
    }
    pub(crate) fn observe_discovered_root_v9(
        &self,
        root: &Path,
        cancel: &CancellationToken,
    ) -> GitResult<()> {
        check_workflow_plan_clock_v9(cancel, self.deadline)?;
        if !cancel.same_instance(&self.cancel)
            || !root.is_absolute()
            || root.as_os_str().len() > 4096
            || !self.directory.starts_with(root)
        {
            return Err(unavailable());
        }
        let mut observed = self.observed_root.lock().map_err(|_| unavailable())?;
        if let Some(old) = observed.as_ref() {
            if old != root {
                return Err(unavailable());
            }
        } else {
            *observed = Some(root.to_path_buf());
        }
        Ok(())
    }
    pub(crate) fn prepare_spec_v9(
        &self,
        spec: &mut CommandSpec,
        window: &RemoteGitReadWindowV1,
        cancel: &CancellationToken,
    ) -> GitResult<()> {
        let runner = window.actual_runner_v1();
        let deadline = window.deadline_v1();
        let root = self.observed_root.lock().map_err(|_| unavailable())?;
        let directory_matches = match root.as_ref() {
            Some(root) => spec.cwd.as_deref() == Some(root.as_path()),
            None => {
                spec.cwd.as_deref() == Some(self.directory.as_path())
                    && spec.args.get(6).is_some_and(|a| a == "rev-parse")
                    && spec.args.len() == 8
                    && [
                        "--absolute-git-dir",
                        "--is-bare-repository",
                        "--show-toplevel",
                    ]
                    .iter()
                    .any(|a| spec.args[7] == *a)
            }
        };
        if !self.runner.same_instance(runner)
            || !self.window.same_operation_v2(window)
            || !cancel.same_instance(&self.cancel)
            || deadline != self.deadline
            || !directory_matches
            || spec.kind != CommandKind::Read
            || !fixed_read_v9(spec, self)
        {
            return Err(unavailable());
        }
        drop(root);
        spec.absolute_deadline_v1 = Some(deadline);
        spec.workflow_environment_v9 = Some(WorkflowGitEnvironmentV9::capture_v9(
            &self.intent,
            runner,
            cancel,
            deadline,
            spec,
        )?);
        Ok(())
    }
}
fn fixed_read_v9(spec: &CommandSpec, context: &WorkflowGitReadEnvironmentV9) -> bool {
    let intent = &context.intent;
    let a = spec.args.as_slice();
    let prefix = [
        "--no-pager",
        "--literal-pathspecs",
        "-c",
        "color.ui=false",
        "-c",
        "diff.autoRefreshIndex=false",
    ];
    if a.len() < prefix.len()
        || !a[..prefix.len()]
            .iter()
            .map(OsString::as_os_str)
            .eq(prefix.map(OsStr::new))
    {
        return false;
    }
    let mut a = &a[prefix.len()..];
    if a.len() >= 2 && a[0] == "-c" && a[1] == "advice.graftFileDeprecated=false" {
        a = &a[2..];
    }
    let original = a.first().is_some_and(|arg| arg == "--no-replace-objects");
    if original {
        a = &a[1..];
    }
    let transport = a.first().is_some_and(|arg| arg == "ls-remote");
    if spec.clear_env
        || spec.env.iter().any(|(key, value)| match key.to_str() {
            Some("GIT_OPTIONAL_LOCKS" | "GIT_TERMINAL_PROMPT") => value != "0",
            Some("GIT_NO_LAZY_FETCH") => value != "1",
            Some("GIT_PAGER") => value != "cat",
            Some("GIT_EDITOR" | "GIT_SEQUENCE_EDITOR") => value != "false",
            Some("GIT_GRAFT_FILE") => !original || value != "/dev/null",
            Some("GIT_ALLOW_PROTOCOL") => transport || !value.is_empty(),
            #[cfg(test)]
            Some("GIT_TRACE2_EVENT") => value.len() > 4096,
            _ => true,
        })
        || [
            "GIT_OPTIONAL_LOCKS",
            "GIT_TERMINAL_PROMPT",
            "GIT_NO_LAZY_FETCH",
            "GIT_PAGER",
            "GIT_EDITOR",
            "GIT_SEQUENCE_EDITOR",
        ]
        .iter()
        .any(|key| !spec.env.contains_key(OsStr::new(key)))
        || (!transport && !spec.env.contains_key(OsStr::new("GIT_ALLOW_PROTOCOL")))
        || (original && !spec.env.contains_key(OsStr::new("GIT_GRAFT_FILE")))
    {
        return false;
    }
    let exact = |values: &[&str]| {
        a.iter()
            .map(OsString::as_os_str)
            .eq(values.iter().map(OsStr::new))
    };
    if !spec.stdin.is_empty() {
        return original
            && exact(&["cat-file", "--batch-check"])
            && spec
                .stdin
                .strip_suffix(b"\n")
                .and_then(|s| std::str::from_utf8(s).ok())
                .is_some_and(original_actor_saved_sha);
    }
    let fixed: &[&[&str]] = &[
        &["rev-parse", "--absolute-git-dir"],
        &["rev-parse", "--is-bare-repository"],
        &["rev-parse", "--show-toplevel"],
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        &["rev-parse", "--is-shallow-repository"],
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
        ],
        &[
            "config",
            "--bool",
            "--default=false",
            "--get",
            "core.sparseCheckout",
        ],
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(upstream)%00%(HEAD)%00",
        ],
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(upstream)%00",
        ],
        &["ls-files", "--stage", "-z"],
        &["config", "--null", "--list"],
        &["diff", "--no-ext-diff", "--no-textconv", "--binary"],
        &["rev-parse", "--verify", "HEAD"],
        &["rev-parse", "--verify", "--end-of-options", "HEAD^{commit}"],
    ];
    if !original && fixed.iter().any(|v| exact(v)) {
        return true;
    }
    // These operands originate only in SAME actual remote observation after
    // numeric/URL/config checks. Validate spelling, never infer an association.
    if !original
        && a.len() == 5
        && a[0] == "remote"
        && a[1] == "get-url"
        && a[2] == "--push"
        && a[3] == "--all"
    {
        return context
            .remote
            .as_ref()
            .is_some_and(|(_, _, name)| a[4] == name.as_str());
    }
    if !original && a.len() == 3 && a[0] == "check-ref-format" && a[1] == "--branch" {
        return a[2].to_str().is_some_and(|s| {
            !s.is_empty()
                && s.len() <= 256
                && !s.starts_with('-')
                && !s.chars().any(|c| c.is_control() || c.is_whitespace())
        });
    }
    if !original && a.len() == 5 && a[0] == "ls-remote" && a[1] == "--refs" && a[2] == "--" {
        return context
            .remote
            .as_ref()
            .is_some_and(|(remote, committed, _)| {
                (a[3] == remote.clone_url() || a[3] == remote.ssh_url())
                    && a[4].to_str().and_then(|s| s.strip_prefix("refs/heads/"))
                        == Some(committed.staged().saved().values().branch())
            });
    }
    if !original && a.first().is_some_and(|s| s == "diff") {
        let mut tail = &a[1..];
        if tail.len() >= 2 && tail[0] == "--numstat" && tail[1] == "-z" {
            tail = &tail[2..];
        }
        let expected = [
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--find-renames",
            "--cached",
        ];
        if tail.len() == expected.len() + 2
            && tail[..expected.len()]
                .iter()
                .map(OsString::as_os_str)
                .eq(expected.map(OsStr::new))
            && tail[expected.len()] == intent.definition().commit_sha.as_str()
            && tail[expected.len() + 1] == "--"
        {
            return true;
        }
    }
    if original
        && a.len() == 4
        && exact(&[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{}^{{commit}}", intent.definition().commit_sha),
        ])
    {
        return true;
    }
    if original && a.len() == 3 && a[0] == "cat-file" && a[1] == "blob" {
        return a[2].to_str().is_some_and(original_actor_saved_sha);
    }
    if original
        && a.len() == 6
        && a[0] == "ls-tree"
        && a[1] == "-z"
        && a[2] == "--full-tree"
        && a[3] == intent.definition().commit_sha.as_str()
        && a[4] == "--"
        && a[5] == intent.definition().workflow.path.as_str()
    {
        return true;
    }
    false
}
