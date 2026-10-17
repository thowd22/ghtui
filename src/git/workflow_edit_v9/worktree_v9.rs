//! Exact absent direct-child destination under the actual private DB parent.
//! A pathname/branch is never a source factory or permission. No directory is
//! created by capture/Preview; only explicit closed Confirm may run Git.
use super::*;
use crate::providers::runner::CommandKind;
use crate::scoped_actions_workflow_edit_v9::{OpenedWorkflowLocalEditV9, WorkflowEditValuesV9};
use std::{
    ffi::CString,
    fs::File,
    os::fd::{AsRawFd, FromRawFd},
    os::unix::fs::MetadataExt,
    path::Component,
    sync::atomic::{AtomicU64, Ordering},
};

static DESTINATION_SERIAL_V9: AtomicU64 = AtomicU64::new(1);

pub(crate) struct WorkflowPreparedWorktreeV9 {
    preview: Arc<crate::scoped_actions_workflow_edit_v9::WorkflowWorktreePreviewV9>,
    cancel: CancellationToken,
    deadline: Instant,
    spec: CommandSpec,
}
impl WorkflowPreparedWorktreeV9 {
    pub(crate) fn prepare_v9(
        request: &crate::scoped_actions_workflow_edit_v9::WorkflowWorktreeConfirmV9,
        repository: &GitRepository,
        status: &RepoStatus,
        effective: Instant,
    ) -> GithubResult<Self> {
        let preview = request.preview();
        let target = preview.target();
        request.check_original_v9(target.opened.binding())?;
        if effective > request.deadline()
            || preview
                .retained_bytes()
                .checked_add(128 * 1024)
                .is_none_or(|n| n > 8 * 1024 * 1024)
        {
            return Err(post_unavailable());
        }
        let purpose = WorkflowWorktreePurposeV9::capture_v9(
            target.clone(),
            request.cancellation(),
            effective,
        )
        .map_err(|_| post_unavailable())?;
        purpose
            .validate(repository)
            .map_err(|_| post_unavailable())?;
        purpose
            .validate_status(status)
            .map_err(|_| post_unavailable())?;
        if !matches!(&preview.plan().mutation.operation, Operation::AddWorktreeBranchAtV9 { path, branch, commit }
            if path == &target.target && branch == target.values.branch() && commit == &target.opened.intent().definition().commit_sha)
        {
            return Err(post_unavailable());
        }
        let args = repository
            .operation_args(&preview.plan().mutation.operation, status)
            .map_err(|_| post_unavailable())?;
        let mut spec = git_command_spec_v9(
            &repository.root,
            args,
            None,
            true,
            &GitReadPolicyV1 {
                timeout: effective.saturating_duration_since(Instant::now()),
                output_limit: 256 * 1024,
                window: None,
                runner: Some(repository.actual_read_runner_v1()),
            },
            Some(effective),
        );
        // Capture complete bounded identity BEFORE any copies and final UID.
        // Marker Arc clones are O(1); its retained allocation is charged once.
        let mask = WorkflowWorktreeMaskV9::capture_v9(request, effective, &spec)?;
        request.check_command_material_v9(mask.retained_bytes_v9())?;
        spec.workflow_worktree_mask_v9 = Some(Arc::new(mask));
        spec.workflow_environment_v9 = Some(
            WorkflowGitEnvironmentV9::capture_v9(
                target.opened.intent(),
                repository.actual_read_runner_v1(),
                request.cancellation(),
                effective,
                &spec,
            )
            .map_err(|_| post_unavailable())?,
        );
        Ok(Self {
            preview: preview.clone(),
            cancel: request.cancellation().clone(),
            deadline: effective,
            spec,
        })
    }
    pub(crate) fn release_v9<'a>(
        self,
        release: &'a crate::scoped_actions_workflow_edit_v9::WorkflowWorktreeReleaseV9<'a>,
    ) -> GithubResult<WorkflowWorktreeWriteCommandV9<'a>> {
        release.check(self.preview.target().opened.binding(), &self.preview)?;
        if release.deadline() != self.deadline {
            return Err(post_unavailable());
        }
        Ok(WorkflowWorktreeWriteCommandV9 {
            prepared: self,
            release,
        })
    }
    #[cfg(test)]
    pub(crate) fn spec_for_mask_test_v9(&self) -> &CommandSpec {
        &self.spec
    }
}

/// A private creation policy, NOT a command/source/permission factory. Its
/// constructor is accessible only alongside the actual prepared Worktree.
pub(crate) struct WorkflowWorktreeMaskV9 {
    preview: Arc<crate::scoped_actions_workflow_edit_v9::WorkflowWorktreePreviewV9>,
    cancel: CancellationToken,
    deadline: Instant,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    cwd: PathBuf,
    kind: CommandKind,
    clear_env: bool,
    coalesce: bool,
    max_output: usize,
    max_timeout: Duration,
}
impl fmt::Debug for WorkflowWorktreeMaskV9 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkflowWorktreeMaskV9(<fixed private child policy redacted>)")
    }
}
impl WorkflowWorktreeMaskV9 {
    fn capture_v9(
        request: &crate::scoped_actions_workflow_edit_v9::WorkflowWorktreeConfirmV9,
        deadline: Instant,
        spec: &CommandSpec,
    ) -> GithubResult<Self> {
        let preview = request.preview();
        let cancel = request.cancellation();
        let target = preview.target();
        let root = &target.opened.binding().origin.original_repository_v1().root;
        if spec.args.len() != 11 || spec.env.len() > 128 {
            return Err(post_unavailable());
        }
        let borrowed = spec.args.iter().try_fold(0usize, |n, s| {
            n.checked_add(s.len())?
                .checked_add(std::mem::size_of::<OsString>())
        });
        let borrowed = borrowed.and_then(|n| {
            spec.env.iter().try_fold(n, |n, (k, v)| {
                n.checked_add(k.len())?
                    .checked_add(v.len())?
                    .checked_add(2 * std::mem::size_of::<OsString>())
            })
        });
        let borrowed = borrowed
            .and_then(|n| n.checked_add(root.as_os_str().len().checked_mul(4)?))
            .and_then(|n| n.checked_add(4 + std::mem::size_of::<Self>()))
            .ok_or_else(post_unavailable)?;
        request.check_command_material_v9(borrowed)?;
        // Borrowed, exact fixed argv checks. No expected argv/body clone is
        // needed merely to establish the pre-copy allocation bound.
        let exact = spec.args[..7].iter().map(OsString::as_os_str).eq([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "color.ui=false",
            "worktree",
            "add",
            "-b",
        ]
        .map(OsStr::new))
            && spec.args[7] == target.values.branch()
            && spec.args[8] == "--"
            && spec.args[9] == target.target.as_os_str()
            && spec.args[10] == target.opened.intent().definition().commit_sha.as_str();
        let resource = format!("git:{}", root.display());
        if borrowed > 64 * 1024
            || spec.program != "git"
            || !exact
            || spec.cwd.as_ref() != Some(root)
            || spec.kind != (CommandKind::Write { resource })
            || !spec.stdin.is_empty()
            || !spec.sensitive_values.is_empty()
            || spec.scoped_read_budget.is_some()
            || spec.original_read_cancellation.is_some()
            || spec.absolute_deadline_v1 != Some(deadline)
            || spec.output_limit == 0
            || spec.output_limit > 256 * 1024
            || spec.timeout.is_zero()
            || spec.workflow_worktree_mask_v9.is_some()
        {
            return Err(post_unavailable());
        }
        // A configured sharing override may undo restrictive creation modes.
        // Unknown/non-default sharing remains refusal, never chmod/repair.
        for record in target.opened.binding().config.split(|b| *b == 0) {
            let Some(newline) = record.iter().position(|b| *b == b'\n') else {
                continue;
            };
            let (key, value) = (&record[..newline], &record[newline + 1..]);
            if key.eq_ignore_ascii_case(b"core.sharedrepository")
                && !matches!(value, b"false" | b"0" | b"umask")
            {
                return Err(post_unavailable());
            }
        }
        target
            .validate_absent_v9(cancel, deadline)
            .map_err(|_| post_unavailable())?;
        Ok(Self {
            preview: preview.clone(),
            cancel: cancel.clone(),
            deadline,
            args: spec.args.clone(),
            env: spec
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            cwd: root.clone(),
            kind: spec.kind.clone(),
            clear_env: spec.clear_env,
            coalesce: spec.coalesce_read,
            max_output: spec.output_limit,
            max_timeout: spec.timeout,
        })
    }
    pub(crate) fn validate_spec_v9(
        &self,
        spec: &CommandSpec,
        cancel: &CancellationToken,
    ) -> GitResult<()> {
        check_workflow_plan_clock_v9(cancel, self.deadline)?;
        if !cancel.same_instance(&self.cancel)
            || !spec
                .workflow_worktree_mask_v9
                .as_ref()
                .is_some_and(|marker| std::ptr::eq(self, marker.as_ref()))
            || spec.program != "git"
            || spec.args != self.args
            || spec.cwd.as_ref() != Some(&self.cwd)
            || spec.kind != self.kind
            || spec.clear_env != self.clear_env
            || spec.coalesce_read != self.coalesce
            || spec.env.len() != self.env.len()
            || !spec
                .env
                .iter()
                .zip(&self.env)
                .all(|((k, v), (old_k, old_v))| k == old_k && v == old_v)
            || !spec.stdin.is_empty()
            || !spec.sensitive_values.is_empty()
            || spec.scoped_read_budget.is_some()
            || spec.original_read_cancellation.is_some()
            || spec.absolute_deadline_v1 != Some(self.deadline)
            || spec.output_limit == 0
            || spec.output_limit > self.max_output
            || spec.timeout.is_zero()
            || spec.timeout > self.max_timeout
            || spec
                .workflow_environment_v9
                .as_ref()
                .is_none_or(|token| token.validate_spec_v9(spec, cancel).is_err())
        {
            return Err(unavailable());
        }
        self.preview
            .target()
            .validate_absent_v9(cancel, self.deadline)
    }
    fn retained_bytes_v9(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.args.capacity() * std::mem::size_of::<OsString>()
            + self.args.iter().map(OsString::capacity).sum::<usize>()
            + self.env.capacity() * std::mem::size_of::<(OsString, OsString)>()
            + self
                .env
                .iter()
                .map(|(k, v)| k.capacity() + v.capacity())
                .sum::<usize>()
            + self.cwd.capacity()
            + match &self.kind {
                CommandKind::Write { resource } => resource.capacity(),
                CommandKind::Read => 0,
            }
    }
}
/// One actual fixed Worktree Write; only the same closed original Core+lock
/// release above can construct it. No generic operation/spec/runner export.
pub(crate) struct WorkflowWorktreeWriteCommandV9<'a> {
    prepared: WorkflowPreparedWorktreeV9,
    release: &'a crate::scoped_actions_workflow_edit_v9::WorkflowWorktreeReleaseV9<'a>,
}
impl WorkflowWorktreeWriteCommandV9<'_> {
    pub(crate) fn validate_original(
        &self,
        runner: &CommandRunner,
        original: &Arc<ScopedActionsWorkflowV2>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        let target = self.prepared.preview.target();
        if deadline != self.prepared.deadline
            || !cancel.same_instance(&self.prepared.cancel)
            || !Arc::ptr_eq(original, target.opened.intent().original())
            || !runner.same_instance(
                target
                    .opened
                    .binding()
                    .origin
                    .original_repository_v1()
                    .actual_read_runner_v1(),
            )
        {
            return Err(post_unavailable());
        }
        self.release
            .check(target.opened.binding(), &self.prepared.preview)?;
        target
            .validate_absent_v9(cancel, deadline)
            .map_err(|_| post_unavailable())
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
            self.prepared.preview.target().opened.intent().original(),
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
        let result = git_command_output_v9(runner.run(self.prepared.spec, cancel))
            .map_err(|_| post_unavailable())?;
        self.prepared
            .preview
            .target()
            .validate_parent_v9(cancel, deadline)
            .map_err(|_| post_unavailable())?;
        if !result.success {
            return Err(post_unavailable());
        }
        Ok(())
    }
}

#[derive(PartialEq, Eq)]
struct ParentStampV9 {
    device: u64,
    inode: u64,
    mode: u32,
    uid: u32,
}
impl ParentStampV9 {
    fn of(file: &File) -> GitResult<Self> {
        let meta = file.metadata()?;
        if !meta.is_dir() {
            return Err(unavailable());
        }
        Ok(Self {
            device: meta.dev(),
            inode: meta.ino(),
            mode: meta.mode(),
            uid: meta.uid(),
        })
    }
}
fn cstring(bytes: &[u8]) -> GitResult<CString> {
    CString::new(bytes).map_err(|_| unavailable())
}
fn descriptor(raw: libc::c_int) -> GitResult<File> {
    if raw < 0 {
        return Err(unavailable());
    }
    // Every successful raw descriptor above/below is transferred exactly once.
    Ok(unsafe { File::from_raw_fd(raw) })
}
fn parent_chain(
    path: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GitResult<(File, Vec<ParentStampV9>)> {
    check_workflow_plan_clock_v9(cancel, deadline)?;
    if !path.is_absolute() || path.as_os_str().len() > 4096 || path.components().count() > 64 {
        return Err(unavailable());
    }
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    let root = cstring(b"/")?;
    let mut file = descriptor(unsafe { libc::open(root.as_ptr(), flags) })?;
    let mut chain = Vec::with_capacity(path.components().count());
    chain.push(ParentStampV9::of(&file)?);
    for component in path.components() {
        check_workflow_plan_clock_v9(cancel, deadline)?;
        match component {
            Component::RootDir => {}
            Component::Normal(part) => {
                let name = cstring(part.as_bytes())?;
                file = descriptor(unsafe { libc::openat(file.as_raw_fd(), name.as_ptr(), flags) })?;
                chain.push(ParentStampV9::of(&file)?);
            }
            _ => return Err(unavailable()),
        }
    }
    let stamp = chain.last().ok_or_else(unavailable)?;
    if stamp.uid != unsafe { libc::geteuid() } || stamp.mode & 0o077 != 0 {
        return Err(unavailable());
    }
    check_workflow_plan_clock_v9(cancel, deadline)?;
    Ok((file, chain))
}

pub(crate) struct WorkflowWorktreeTargetV9 {
    opened: Arc<OpenedWorkflowLocalEditV9>,
    values: Arc<WorkflowEditValuesV9>,
    parent_path: PathBuf,
    parent: File,
    chain: Vec<ParentStampV9>,
    target: PathBuf,
    name: CString,
}
impl fmt::Debug for WorkflowWorktreeTargetV9 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "WorkflowWorktreeTargetV9(<actual absent private child; no write/source permission>)",
        )
    }
}
impl WorkflowWorktreeTargetV9 {
    pub(crate) fn capture_default_v9(
        opened: Arc<OpenedWorkflowLocalEditV9>,
        values: Arc<WorkflowEditValuesV9>,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        check_workflow_plan_clock_v9(cancel, deadline).map_err(|_| post_unavailable())?;
        if opened.binding().parent.is_some() {
            return Err(post_unavailable());
        }
        opened.validate_local(database, cancel)?;
        let parent = database
            .parent()
            .filter(|path| path.is_absolute())
            .ok_or_else(post_unavailable)?;
        if parent.as_os_str().len() > 4096 || parent.components().count() > 64 {
            return Err(post_unavailable());
        }
        let parent_path = parent.to_path_buf();
        let (parent, chain) =
            parent_chain(&parent_path, cancel, deadline).map_err(|_| post_unavailable())?;
        let serial = DESTINATION_SERIAL_V9
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| post_unavailable())?;
        let name =
            cstring(format!(".ghtui-workflow-v9-{}-{serial}", std::process::id()).as_bytes())
                .map_err(|_| post_unavailable())?;
        if parent_path
            .as_os_str()
            .len()
            .checked_add(name.as_bytes().len() + 1)
            .is_none_or(|n| n > 4096)
        {
            return Err(post_unavailable());
        }
        let target = parent_path.join(OsStr::from_bytes(name.as_bytes()));
        let value = Arc::new(Self {
            opened,
            values,
            parent_path,
            parent,
            chain,
            target,
            name,
        });
        value
            .validate_absent_v9(cancel, deadline)
            .map_err(|_| post_unavailable())?;
        Ok(value)
    }
    pub(crate) fn validate_parent_v9(
        &self,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GitResult<()> {
        check_workflow_plan_clock_v9(cancel, deadline)?;
        self.opened.binding().check_origin_v9(cancel)?;
        let (fresh, chain) = parent_chain(&self.parent_path, cancel, deadline)?;
        if chain != self.chain
            || self.chain.last() != Some(&ParentStampV9::of(&self.parent)?)
            || self.chain.last() != Some(&ParentStampV9::of(&fresh)?)
        {
            return Err(unavailable());
        }
        check_workflow_plan_clock_v9(cancel, deadline)
    }
    pub(crate) fn validate_absent_v9(
        &self,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GitResult<()> {
        self.validate_parent_v9(cancel, deadline)?;
        let mut meta = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe {
            libc::fstatat(
                self.parent.as_raw_fd(),
                self.name.as_ptr(),
                meta.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } == 0
            || std::io::Error::last_os_error().raw_os_error() != Some(libc::ENOENT)
        {
            return Err(unavailable());
        }
        check_workflow_plan_clock_v9(cancel, deadline)
    }
    pub(crate) fn opened(&self) -> &Arc<OpenedWorkflowLocalEditV9> {
        &self.opened
    }
    pub(crate) fn values(&self) -> &Arc<WorkflowEditValuesV9> {
        &self.values
    }
    pub(crate) fn path(&self) -> &Path {
        &self.target
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.parent_path.capacity()
            + self.target.capacity()
            + self.name.as_bytes_with_nul().len()
            + self.chain.capacity() * std::mem::size_of::<ParentStampV9>()
    }
    fn created_root_v9(&self, cancel: &CancellationToken, deadline: Instant) -> GitResult<File> {
        self.validate_parent_v9(cancel, deadline)?;
        let file = descriptor(unsafe {
            libc::openat(
                self.parent.as_raw_fd(),
                self.name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        let stamp = ParentStampV9::of(&file)?;
        if stamp.uid != unsafe { libc::geteuid() } || stamp.mode & 0o022 != 0 {
            return Err(unavailable());
        }
        self.validate_parent_v9(cancel, deadline)?;
        Ok(file)
    }
}

#[derive(Clone, Copy)]
enum WorktreeReadV9 {
    Discover(u8),
    Status(u8),
    Head,
    List,
}
/// Fixed Worktree-phase reads only. Private construction retains the actual
/// target/parent FD and original binding; no raw argv/runner getter exists.
pub(crate) struct WorkflowWorktreeReadCommandV9 {
    target: Arc<WorkflowWorktreeTargetV9>,
    kind: WorktreeReadV9,
    cancel: CancellationToken,
    deadline: Instant,
}
impl WorkflowWorktreeReadCommandV9 {
    pub(crate) fn role(&self) -> WorkflowGitRoleV9 {
        match self.kind {
            WorktreeReadV9::Discover(n) => WorkflowGitRoleV9::Discover(n),
            WorktreeReadV9::Status(n) => WorkflowGitRoleV9::Status(n),
            WorktreeReadV9::Head => WorkflowGitRoleV9::ResolveHead,
            WorktreeReadV9::List => WorkflowGitRoleV9::WorktreeList,
        }
    }
    pub(crate) fn validate_original(
        &self,
        runner: &CommandRunner,
        original: &Arc<ScopedActionsWorkflowV2>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        if deadline != self.deadline
            || !cancel.same_instance(&self.cancel)
            || !Arc::ptr_eq(original, self.target.opened.intent().original())
            || !runner.same_instance(
                self.target
                    .opened
                    .binding()
                    .origin
                    .original_repository_v1()
                    .actual_read_runner_v1(),
            )
        {
            return Err(post_unavailable());
        }
        self.target
            .created_root_v9(cancel, deadline)
            .map_err(|_| post_unavailable())?;
        Ok(())
    }
    pub(crate) fn execute_bounded(
        self,
        runner: &CommandRunner,
        cap: usize,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowGitOutputV9> {
        self.validate_original(
            runner,
            self.target.opened.intent().original(),
            cancel,
            deadline,
        )?;
        if !(1..=256 * 1024).contains(&cap) {
            return Err(post_unavailable());
        }
        let args: &[&str] = match self.kind {
            WorktreeReadV9::Discover(0) => &["rev-parse", "--absolute-git-dir"],
            WorktreeReadV9::Discover(1) => &["rev-parse", "--is-bare-repository"],
            WorktreeReadV9::Discover(2) => &["rev-parse", "--show-toplevel"],
            WorktreeReadV9::Discover(3) => {
                &["rev-parse", "--path-format=absolute", "--git-common-dir"]
            }
            WorktreeReadV9::Status(0) => &[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
            ],
            WorktreeReadV9::Status(1) => &["rev-parse", "--is-shallow-repository"],
            WorktreeReadV9::Status(2) => &[
                "config",
                "--bool",
                "--default=false",
                "--get",
                "core.sparseCheckout",
            ],
            WorktreeReadV9::Status(3) => &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(upstream)%00%(HEAD)%00",
            ],
            WorktreeReadV9::Head => &[
                "--no-replace-objects",
                "rev-parse",
                "--verify",
                "--end-of-options",
                "HEAD^{commit}",
            ],
            WorktreeReadV9::List => &["worktree", "list", "--porcelain", "-z"],
            _ => return Err(post_unavailable()),
        };
        let directory: &Path = if matches!(self.kind, WorktreeReadV9::List) {
            &self
                .target
                .opened
                .binding()
                .origin
                .original_repository_v1()
                .root
        } else {
            self.target.path()
        };
        let mut spec = git_command_spec_v9(
            directory,
            args.iter().map(|arg| OsString::from(*arg)),
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
                self.target.opened.intent(),
                runner,
                cancel,
                deadline,
                &spec,
            )
            .map_err(|_| post_unavailable())?,
        );
        let output =
            git_command_output_v9(runner.run(spec, cancel)).map_err(|_| post_unavailable())?;
        self.validate_original(
            runner,
            self.target.opened.intent().original(),
            cancel,
            deadline,
        )?;
        if !output.success {
            return Err(post_unavailable());
        }
        Ok(WorkflowGitOutputV9 { output })
    }
}

/// Issues a depth-one binding ONLY after the actual original paid Write and
/// fixed discover4/status4/HEAD/list outputs. It is not a remote permission.
pub(crate) struct WorkflowObservedWorktreeV9 {
    binding: Arc<WorkflowLocalBindingV9>,
    file: Arc<WorkflowWorkingFileV9>,
    status: RepoStatus,
}
impl WorkflowObservedWorktreeV9 {
    pub(crate) fn into_parts_v9(
        self,
    ) -> (
        Arc<WorkflowLocalBindingV9>,
        Arc<WorkflowWorkingFileV9>,
        RepoStatus,
    ) {
        (self.binding, self.file, self.status)
    }
}
impl WorkflowWorktreeTargetV9 {
    pub(crate) fn observe_after_attempt_v9(
        self: &Arc<Self>,
        provider: &GithubProvider,
        capture: &mut WorkflowPostCaptureV9,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowObservedWorktreeV9> {
        if capture.phase() != WorkflowEditPhaseV9::Worktree
            || capture.consumed() != 1
            || self.opened.binding().parent.is_some()
        {
            return Err(post_unavailable());
        }
        let deadline = capture.deadline();
        let root_descriptor = self
            .created_root_v9(cancel, deadline)
            .map_err(|_| post_unavailable())?;
        let mut read = |kind| {
            capture
                .worktree_git_v9(
                    provider,
                    WorkflowWorktreeReadCommandV9 {
                        target: self.clone(),
                        kind,
                        cancel: cancel.clone(),
                        deadline,
                    },
                    cancel,
                )?
                .bytes()
        };
        let discovery = [
            read(WorktreeReadV9::Discover(0))?,
            read(WorktreeReadV9::Discover(1))?,
            read(WorktreeReadV9::Discover(2))?,
            read(WorktreeReadV9::Discover(3))?,
        ];
        let runner = self
            .opened
            .binding()
            .origin
            .original_repository_v1()
            .actual_read_runner_v1()
            .clone();
        let repository = GitRepository::discovered_from_bytes_v9(
            [&discovery[0], &discovery[1], &discovery[2], &discovery[3]],
            cancel.clone(),
            None,
            Some(runner),
        );
        let original = self.opened.binding().origin.original_repository_v1();
        if repository.root != self.target
            || repository.bare
            || repository.git_dir == original.git_dir
            || repository.common_dir != original.common_dir
            || !repository.git_dir.is_absolute()
            || !repository.common_dir.is_absolute()
        {
            return Err(post_unavailable());
        }
        let origin =
            OriginalLocalActorOriginV1::capture(&repository).map_err(|_| post_unavailable())?;
        let root_meta = root_descriptor.metadata().map_err(|_| post_unavailable())?;
        if origin.source().root_file_identity_v3() != (root_meta.dev(), root_meta.ino()) {
            return Err(post_unavailable());
        }
        let mut status =
            parse_status(&read(WorktreeReadV9::Status(0))?).map_err(|_| post_unavailable())?;
        status.operation = repository.sequencer();
        status.shallow = text(trim_line(&read(WorktreeReadV9::Status(1))?)) == "true";
        status.sparse = read(WorktreeReadV9::Status(2))? == b"true\n";
        let configured =
            std::env::var_os("GIT_REPLACE_REF_BASE").unwrap_or_else(|| "refs/replace".into());
        let references = GitRepository::references_from_bytes_v9(&read(WorktreeReadV9::Status(3))?)
            .map_err(|_| post_unavailable())?;
        (status.replacement_refs, status.grafted) =
            repository.history_override_flags_v9(&references, &replacement_prefix(&configured));
        let head = read(WorktreeReadV9::Head)?;
        let trees = parse_worktrees_v9(&read(WorktreeReadV9::List)?);
        let expected = &self.opened.intent().definition().commit_sha;
        let branch = format!("refs/heads/{}", self.values.branch());
        if !status.clean()
            || status.operation.is_some()
            || status.has_history_overrides()
            || status.sparse
            || status.head.as_deref() != Some(expected.as_str())
            || trim_line(&head) != expected.as_bytes()
            || status.branch.as_deref() != Some(self.values.branch())
            || trees.iter().filter(|tree| tree.path == self.target).count() != 1
            || !trees.iter().any(|tree| {
                tree.path == self.target
                    && tree.head.as_deref() == Some(expected.as_str())
                    && tree.branch.as_deref() == Some(branch.as_str())
                    && !tree.bare
                    && !tree.locked
                    && !tree.prunable
            })
        {
            return Err(post_unavailable());
        }
        let parent = self.opened.binding();
        // No new config authority: retained old bytes are a restriction only.
        // Every later actual Save/Stage/Commit reads+compares fresh full config.
        let binding = Arc::new(WorkflowLocalBindingV9 {
            intent: parent.intent.clone(),
            mapping: parent.mapping.clone(),
            origin,
            root_descriptor,
            parent: Some(parent.clone()),
            isolated_branch: Some(self.values.branch().to_owned()),
            config: parent.config.clone(),
            mode: parent.mode.clone(),
            blob: parent.blob.clone(),
        });
        let file = binding
            .observe_working_file_v9(cancel, deadline)
            .map_err(|_| post_unavailable())?;
        if file.content() != self.opened.intent().definition().yaml.as_bytes()
            || !file.mode_matches_saved_v9(&parent.mode)
        {
            return Err(post_unavailable());
        }
        binding
            .check_origin_v9(cancel)
            .map_err(|_| post_unavailable())?;
        self.validate_parent_v9(cancel, deadline)
            .map_err(|_| post_unavailable())?;
        Ok(WorkflowObservedWorktreeV9 {
            binding,
            file,
            status,
        })
    }
}

/// Closed exact planner input, constructed only from actual current matching
/// bytes plus the captured absent private destination. No caller Operation.
pub(crate) struct WorkflowWorktreePurposeV9 {
    target: Arc<WorkflowWorktreeTargetV9>,
    cancel: CancellationToken,
    deadline: Instant,
}
impl WorkflowWorktreePurposeV9 {
    pub(crate) fn capture_v9(
        target: Arc<WorkflowWorktreeTargetV9>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GitResult<Self> {
        target.validate_absent_v9(cancel, deadline)?;
        Ok(Self {
            target,
            cancel: cancel.clone(),
            deadline,
        })
    }
    pub(crate) fn validate(&self, repository: &GitRepository) -> GitResult<()> {
        self.target
            .validate_absent_v9(&self.cancel, self.deadline)?;
        let binding = self.target.opened.binding();
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
        Ok(())
    }
    pub(crate) fn validate_status(&self, status: &RepoStatus) -> GitResult<()> {
        if status.head.as_deref()
            != Some(self.target.opened.intent().definition().commit_sha.as_str())
            || status.operation.is_some()
            || status.has_history_overrides()
            || status.conflicts() != 0
            || status.sparse
        {
            return Err(unavailable());
        }
        Ok(())
    }
    /// The SAME isolated-worktree purpose may preserve unrelated staged
    /// bytes in the original tree. Direct-root Save/Stage/Commit retain their
    /// stricter existing path guard. Exact original fingerprint/config and
    /// every independent worktree/source/status check still apply here.
    pub(crate) fn check_fresh_plan_repository_v9(
        &self,
        repository: &GitRepository,
    ) -> GitResult<(RepoStatus, WorkspaceFingerprint)> {
        self.validate(repository)?;
        let status = repository.status()?;
        self.validate_status(&status)?;
        let binding = self.target.opened.binding();
        if !binding.branch_matches_v9(&status) {
            return Err(unavailable());
        }
        let (observed, config) = repository.fingerprint_and_config_v9(256 * 1024)?;
        if config != binding.config {
            return Err(unavailable());
        }
        Ok((status, observed))
    }
    pub(crate) fn operation(&self) -> Operation {
        Operation::AddWorktreeBranchAtV9 {
            path: self.target.target.clone(),
            branch: self.target.values.branch().into(),
            commit: self.target.opened.intent().definition().commit_sha.clone(),
        }
    }
    pub(in crate::git) fn original_config(&self) -> &[u8] {
        &self.target.opened.binding().config
    }
    pub(crate) fn target(&self) -> &Arc<WorkflowWorktreeTargetV9> {
        &self.target
    }
}
