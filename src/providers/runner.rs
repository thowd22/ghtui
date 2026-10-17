//! Bounded worker-side process execution with no shell interpretation.
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[path = "runner_issue_link_v87.rs"]
mod issue_link_v87;
#[path = "runner/notification_conditional_v63.rs"]
mod notification_conditional_v63;
#[path = "runner_notification_recovery_v63.rs"]
mod notification_recovery_v63;
#[path = "runner_review_recovery_v11.rs"]
mod review_recovery_v11;
pub(in crate::providers) use issue_link_v87::IssueLinkMutationRunV87;
#[path = "scoped_read_budget_v1.rs"]
mod scoped_read_budget_v1;
#[path = "runner/workflow_branch_v9.rs"]
mod workflow_branch_v9;
#[path = "runner/workflow_environment_v9.rs"]
mod workflow_environment_v9;
pub(crate) use notification_conditional_v63::NotificationConditionalOutputV63;
pub use scoped_read_budget_v1::ScopedReadAccountingV1;
pub(crate) use scoped_read_budget_v1::ScopedReadBudgetV1;
pub use workflow_environment_v9::initialize_workflow_environment_v9;
pub(crate) use workflow_environment_v9::WorkflowProcessEnvironmentV9;

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    /// Allocation identity only, never permission or publication authority.
    pub(crate) fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CommandKind {
    Read,
    Write { resource: String },
}

#[derive(Clone)]
pub struct CommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<OsString, OsString>,
    pub clear_env: bool,
    pub stdin: Vec<u8>,
    pub timeout: Duration,
    /// Bounds only; never a read or write permission. Setup and lock waiting
    /// cannot extend the caller's original absolute phase deadline.
    pub(crate) absolute_deadline_v1: Option<Instant>,
    pub output_limit: usize,
    pub kind: CommandKind,
    /// Only ordinary reads may share an in-flight response. Security-boundary
    /// observations must launch after their own boundary, not join an older read.
    pub coalesce_read: bool,
    pub(crate) scoped_read_budget: Option<Arc<ScopedReadBudgetV1>>,
    /// Bounds-only original READ token; no permission or principal authority.
    pub(crate) original_read_cancellation: Option<CancellationToken>,
    /// Values to remove from diagnostic output; stdin and environment are never logged.
    pub sensitive_values: Vec<String>,
    /// Restriction-only child creation mask. Only the actual fixed W9
    /// Worktree planner can mint this private marker; old specs remain None.
    pub(crate) workflow_worktree_mask_v9:
        Option<Arc<crate::git::workflow_edit_v9::WorkflowWorktreeMaskV9>>,
    /// Only actual fixed W9 Git factories can mint this sealed replay token.
    pub(crate) workflow_environment_v9:
        Option<Arc<crate::git::workflow_edit_v9::WorkflowGitEnvironmentV9>>,
}
impl CommandSpec {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            clear_env: false,
            stdin: Vec::new(),
            timeout: Duration::from_secs(30),
            absolute_deadline_v1: None,
            output_limit: 4 * 1024 * 1024,
            kind: CommandKind::Read,
            coalesce_read: true,
            scoped_read_budget: None,
            original_read_cancellation: None,
            sensitive_values: Vec::new(),
            workflow_worktree_mask_v9: None,
            workflow_environment_v9: None,
        }
    }
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }
    pub fn cwd(mut self, path: impl Into<PathBuf>) -> Self {
        self.cwd = Some(path.into());
        self
    }
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
    pub fn input(mut self, input: impl Into<Vec<u8>>) -> Self {
        self.stdin = input.into();
        self
    }
    pub fn write(mut self, resource: impl Into<String>) -> Self {
        self.kind = CommandKind::Write {
            resource: resource.into(),
        };
        self
    }
    /// Preserve all process bounds while bypassing older in-flight read results.
    pub fn fresh_read(mut self) -> Self {
        self.coalesce_read = false;
        self
    }
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    fn original_deadline_expired_v1(&self) -> bool {
        self.absolute_deadline_v1
            .is_some_and(|deadline| Instant::now() >= deadline)
    }
}

#[derive(Clone, Debug)]
pub struct CommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
    pub elapsed: Duration,
    pub capture: CommandCapture,
}
/// Kept pipe bytes BEFORE redaction/sanitization, not discarded or wire bytes.
/// Only the runner can construct this metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandCapture {
    stdout_bytes: usize,
    stderr_bytes: usize,
}
impl CommandCapture {
    pub fn stdout_bytes(&self) -> usize {
        self.stdout_bytes
    }
    pub fn stderr_bytes(&self) -> usize {
        self.stderr_bytes
    }
    pub fn total_bytes(&self) -> usize {
        self.stdout_bytes.saturating_add(self.stderr_bytes)
    }
}
impl CommandOutput {
    pub fn stdout_text(&self) -> String {
        redact(
            &sanitize_terminal(&String::from_utf8_lossy(&self.stdout)),
            &[],
        )
    }
    pub fn stderr_text(&self) -> String {
        redact(
            &sanitize_terminal(&String::from_utf8_lossy(&self.stderr)),
            &[],
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureKind {
    MissingProgram,
    Spawn,
    Exit,
    Timeout,
    Cancelled,
    OutputLimit,
    ReconciliationRequired,
}
#[derive(Clone, Debug)]
pub struct CommandFailure {
    pub kind: FailureKind,
    pub message: String,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub outcome_unknown: bool,
}
impl std::fmt::Display for CommandFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for CommandFailure {}

type RunResult = Result<CommandOutput, CommandFailure>;
type ReadGate = Arc<(Mutex<Option<RunResult>>, Condvar)>;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ReadKey {
    program: OsString,
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
    env: BTreeMap<OsString, OsString>,
    clear_env: bool,
    stdin: Vec<u8>,
    timeout: Duration,
    absolute_deadline_v1: Option<Instant>,
    output_limit: usize,
    sensitive_values: Vec<String>,
}
impl From<&CommandSpec> for ReadKey {
    fn from(s: &CommandSpec) -> Self {
        Self {
            program: s.program.clone(),
            args: s.args.clone(),
            cwd: s.cwd.clone(),
            env: s.env.clone(),
            clear_env: s.clear_env,
            stdin: s.stdin.clone(),
            timeout: s.timeout,
            absolute_deadline_v1: s.absolute_deadline_v1,
            output_limit: s.output_limit,
            sensitive_values: s.sensitive_values.clone(),
        }
    }
}
#[derive(Default)]
struct ResourceState {
    ambiguous: bool,
    generation: u64,
}
#[derive(Default)]
struct RunnerState {
    reads: HashMap<ReadKey, ReadGate>,
    writes: HashMap<String, Arc<Mutex<ResourceState>>>,
}
#[derive(Clone, Default)]
pub struct CommandRunner {
    state: Arc<Mutex<RunnerState>>,
}
impl std::fmt::Debug for CommandRunner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CommandRunner { shared bounded execution state }")
    }
}
impl CommandRunner {
    /// Observations are valid only for the exact shared state that produced them.
    pub fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }
    pub fn run(&self, spec: CommandSpec, cancellation: &CancellationToken) -> RunResult {
        if let Some(environment) = &spec.workflow_environment_v9 {
            environment
                .validate_runner_v9(self)
                .and_then(|_| environment.validate_spec_v9(&spec, cancellation))
                .map_err(|_| {
                    failure(
                        FailureKind::ReconciliationRequired,
                        "Original W9 producer/spec changed; no child launched",
                        false,
                    )
                })?;
        }
        if let Some(budget) = spec.scoped_read_budget.clone() {
            return budget.run(spec, cancellation);
        }
        if matches!(spec.kind, CommandKind::Read) && !spec.coalesce_read {
            return execute(&spec, cancellation, None);
        }
        let started = Instant::now();
        match &spec.kind {
            CommandKind::Read => {
                let key = ReadKey::from(&spec);
                let (gate, owner) = {
                    let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                    if let Some(gate) = state.reads.get(&key) {
                        (gate.clone(), false)
                    } else {
                        let gate = Arc::new((Mutex::new(None), Condvar::new()));
                        state.reads.insert(key.clone(), gate.clone());
                        (gate, true)
                    }
                };
                if owner {
                    let result = execute(&spec, cancellation, None);
                    *gate.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(result.clone());
                    self.state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .reads
                        .remove(&key);
                    gate.1.notify_all();
                    result
                } else {
                    let mut result = gate.0.lock().unwrap_or_else(|p| p.into_inner());
                    while result.is_none() {
                        if cancellation.is_cancelled() {
                            return Err(failure(
                                FailureKind::Cancelled,
                                "Read wait cancelled",
                                false,
                            ));
                        }
                        if started.elapsed() >= spec.timeout || spec.original_deadline_expired_v1()
                        {
                            return Err(failure(
                                FailureKind::Timeout,
                                "Read wait exceeded its deadline",
                                false,
                            ));
                        }
                        result = gate
                            .1
                            .wait_timeout(result, Duration::from_millis(10))
                            .unwrap_or_else(|p| p.into_inner())
                            .0;
                    }
                    let completed = result.clone().expect("completed coalesced read");
                    drop(result);
                    if completed
                        .as_ref()
                        .is_err_and(|e| e.kind == FailureKind::Cancelled)
                        && !cancellation.is_cancelled()
                    {
                        // An independent subscriber must not inherit the owner's cancellation.
                        let mut remaining = spec;
                        remaining.timeout = remaining.timeout.saturating_sub(started.elapsed());
                        self.run(remaining, cancellation)
                    } else {
                        completed
                    }
                }
            }
            CommandKind::Write { resource } => {
                let lock = {
                    let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                    state.writes.entry(resource.clone()).or_default().clone()
                };
                let waiting = Instant::now();
                let mut state = loop {
                    if cancellation.is_cancelled() {
                        return Err(failure(
                            FailureKind::Cancelled,
                            "Write wait cancelled",
                            false,
                        ));
                    }
                    if waiting.elapsed() >= spec.timeout || spec.original_deadline_expired_v1() {
                        return Err(failure(
                            FailureKind::Timeout,
                            "Write wait exceeded its deadline",
                            false,
                        ));
                    }
                    match lock.try_lock() {
                        Ok(state) => break state,
                        Err(std::sync::TryLockError::Poisoned(p)) => break p.into_inner(),
                        Err(std::sync::TryLockError::WouldBlock) => {
                            thread::sleep(Duration::from_millis(5))
                        }
                    }
                };
                if state.ambiguous {
                    return Err(failure(FailureKind::ReconciliationRequired, "Previous write outcome is unknown; refresh the resource before deciding whether to retry", true));
                }
                let mut remaining = spec.clone();
                remaining.timeout = remaining.timeout.saturating_sub(waiting.elapsed());
                state.generation = state.generation.wrapping_add(1);
                let result = execute(&remaining, cancellation, None);
                if result.as_ref().is_err_and(|e| e.outcome_unknown) {
                    state.ambiguous = true;
                }
                result
            }
        }
    }
    /// A response may be unusable even after process exit 0.
    pub fn mark_ambiguous(&self, resource: &str) {
        let lock = self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .writes
            .entry(resource.to_owned())
            .or_default()
            .clone();
        let mut state = lock.lock().unwrap_or_else(|p| p.into_inner());
        state.ambiguous = true;
        state.generation = state.generation.wrapping_add(1);
    }
    /// A fresh inspection records this version before and after reading affected state.
    pub fn resource_generation(&self, resource: &str) -> u64 {
        let lock = self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .writes
            .entry(resource.to_owned())
            .or_default()
            .clone();
        let state = lock.lock().unwrap_or_else(|p| p.into_inner());
        state.generation
    }
    /// Old observations must not clear the barrier established by a later write.
    pub fn mark_reconciled_generation(&self, resource: &str, generation: u64) -> bool {
        let lock = self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .writes
            .entry(resource.to_owned())
            .or_default()
            .clone();
        let mut state = lock.lock().unwrap_or_else(|p| p.into_inner());
        if state.generation != generation {
            return false;
        }
        state.ambiguous = false;
        true
    }
    /// Atomically acknowledge related resource versions; no partial barrier clearing.
    pub fn mark_reconciled_generations(&self, versions: &[(String, u64)]) -> bool {
        let mut ordered = versions.to_vec();
        ordered.sort_by(|a, b| a.0.cmp(&b.0));
        if ordered.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return false;
        }
        let locks = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            ordered
                .iter()
                .map(|(key, _)| state.writes.entry(key.clone()).or_default().clone())
                .collect::<Vec<_>>()
        };
        let mut guards = locks
            .iter()
            .map(|lock| lock.lock().unwrap_or_else(|p| p.into_inner()))
            .collect::<Vec<_>>();
        if guards
            .iter()
            .zip(&ordered)
            .any(|(state, (_, version))| state.generation != *version)
        {
            return false;
        }
        for state in &mut guards {
            state.ambiguous = false;
        }
        true
    }
    /// Only call after fetching and inspecting current remote state.
    pub fn mark_reconciled(&self, resource: &str) {
        if let Some(lock) = self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .writes
            .get(resource)
        {
            lock.lock().unwrap_or_else(|p| p.into_inner()).ambiguous = false;
        }
    }
}
fn failure(kind: FailureKind, message: impl Into<String>, unknown: bool) -> CommandFailure {
    CommandFailure {
        kind,
        message: message.into(),
        exit_code: None,
        stdout: Vec::new(),
        stderr: Vec::new(),
        outcome_unknown: unknown,
    }
}

// A child may exit while another process still holds an inherited pipe. All
// three owned I/O workers must remain stoppable at the original command bound.
#[cfg(unix)]
fn nonblocking_pipe(pipe: &impl AsRawFd) -> std::io::Result<()> {
    let fd = pipe.as_raw_fd();
    // SAFETY: the live owned pipe retains this descriptor through both calls;
    // F_GETFL/F_SETFL change only its open-file status flags, never ownership.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

fn execute(
    spec: &CommandSpec,
    cancellation: &CancellationToken,
    measured: Option<&mut scoped_read_budget_v1::ReadCaptureClaimV1>,
) -> RunResult {
    execute_mode_v63(spec, cancellation, measured, ExecutePolicyV63::Strict)
}
#[derive(Clone, Copy)]
enum ExecutePolicyV63 {
    Strict,
    NotificationConditionalGet,
    WorkflowBranchGetV9,
}
fn execute_mode_v63(
    spec: &CommandSpec,
    cancellation: &CancellationToken,
    mut measured: Option<&mut scoped_read_budget_v1::ReadCaptureClaimV1>,
    policy: ExecutePolicyV63,
) -> RunResult {
    if cancellation.is_cancelled()
        || spec
            .original_read_cancellation
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
    {
        return Err(failure(
            FailureKind::Cancelled,
            "Command cancelled before launch",
            false,
        ));
    }
    if spec.timeout.is_zero() || spec.original_deadline_expired_v1() {
        return Err(failure(
            FailureKind::Timeout,
            "Command deadline expired before launch",
            false,
        ));
    }
    // Validate before Command/env/CString copies or Worktree pre_exec setup.
    if let Some(environment) = &spec.workflow_environment_v9 {
        environment
            .validate_spec_v9(spec, cancellation)
            .map_err(|_| {
                failure(
                    FailureKind::ReconciliationRequired,
                    "Original private W9 Git command changed; no child launched",
                    false,
                )
            })?;
    }
    let start = Instant::now();
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    if let Some(environment) = &spec.workflow_environment_v9 {
        environment
            .configure_command_v9(spec, cancellation, &mut command)
            .map_err(|_| {
                failure(
                    FailureKind::ReconciliationRequired,
                    "Original private W9 Git environment unavailable; no child launched",
                    false,
                )
            })?;
    } else {
        if spec.clear_env {
            command.env_clear();
        }
        command.envs(&spec.env);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        if let Some(mask) = &spec.workflow_worktree_mask_v9 {
            mask.validate_spec_v9(spec, cancellation).map_err(|_| {
                failure(
                    FailureKind::ReconciliationRequired,
                    "Original private Worktree command identity changed; no child launched",
                    false,
                )
            })?;
            // ONLY this marked child, after fork and before exec. umask is
            // async-signal-safe; the parent's process-wide mask is untouched.
            unsafe {
                command.pre_exec(|| {
                    libc::umask(0o077);
                    Ok(())
                });
            }
        }
    }
    // Original cancellation may occur during process setup, before spawn.
    if cancellation.is_cancelled()
        || spec
            .original_read_cancellation
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
    {
        return Err(failure(
            FailureKind::Cancelled,
            "Original read cancelled before spawn",
            false,
        ));
    }
    if spec.original_deadline_expired_v1() || start.elapsed() >= spec.timeout {
        return Err(failure(
            FailureKind::Timeout,
            "Command deadline expired during setup",
            false,
        ));
    }
    if let Some(claim) = measured.as_mut() {
        claim.before_spawn(cancellation)?;
    }
    let mut child = command.spawn().map_err(|e| {
        failure(
            if e.kind() == std::io::ErrorKind::NotFound {
                FailureKind::MissingProgram
            } else {
                FailureKind::Spawn
            },
            format!("Could not start {}: {e}", spec.program.to_string_lossy()),
            false,
        )
    })?;
    let group = child.id();
    if let Some(claim) = measured.as_mut() {
        if let Err(error) = claim.started() {
            terminate(group);
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    }
    let mut stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    #[cfg(unix)]
    if let Err(error) = nonblocking_pipe(&stdin)
        .and_then(|_| nonblocking_pipe(&stdout))
        .and_then(|_| nonblocking_pipe(&stderr))
    {
        terminate(group);
        let _ = child.kill();
        let _ = child.wait();
        return Err(failure(
            FailureKind::Spawn,
            format!("Could not configure bounded command pipes: {error}"),
            matches!(spec.kind, CommandKind::Write { .. }),
        ));
    }
    let input = spec.stdin.clone();
    let io_stop = Arc::new(AtomicBool::new(false));
    let input_stop = io_stop.clone();
    let input_thread = thread::spawn(move || {
        let mut remaining = input.as_slice();
        while !remaining.is_empty() && !input_stop.load(Ordering::Acquire) {
            match stdin.write(remaining) {
                Ok(0) => break,
                Ok(count) => remaining = &remaining[count..],
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                }
                Err(_) => break,
            }
        }
    });
    let size = Arc::new(AtomicUsize::new(0));
    let limit = spec.output_limit;
    let read_pipe = |mut pipe: Box<dyn Read + Send>, size: Arc<AtomicUsize>| {
        let stop = io_stop.clone();
        thread::spawn(move || {
            let mut output = Vec::new();
            let mut buffer = [0; 8192];
            loop {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                match pipe.read(&mut buffer) {
                    Ok(0) => break,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                    Ok(n) => {
                        let before = size.fetch_add(n, Ordering::Relaxed);
                        let keep = n.min(limit.saturating_sub(before));
                        output.extend_from_slice(&buffer[..keep]);
                        if before.saturating_add(n) > limit {
                            break;
                        }
                    }
                }
            }
            output
        })
    };
    let stdout_thread = read_pipe(Box::new(stdout), size.clone());
    let stderr_thread = read_pipe(Box::new(stderr), size.clone());
    let mut stopped = None;
    let mut status = None;
    loop {
        if cancellation.is_cancelled()
            || spec
                .original_read_cancellation
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
            || measured
                .as_ref()
                .is_some_and(|claim| claim.original_cancelled())
        {
            stopped = Some(FailureKind::Cancelled);
        } else if start.elapsed() >= spec.timeout
            || spec.original_deadline_expired_v1()
            || measured
                .as_ref()
                .is_some_and(|claim| claim.original_deadline_expired())
        {
            stopped = Some(FailureKind::Timeout);
        } else if size.load(Ordering::Relaxed) > limit {
            stopped = Some(FailureKind::OutputLimit);
        }
        if stopped.is_some() {
            terminate(group);
            let _ = child.kill();
            if status.is_none() {
                status = child.wait().ok();
            }
            break;
        }
        if status.is_none() {
            match child.try_wait() {
                Ok(Some(exit)) => {
                    status = Some(exit);
                    // Close ordinary owned-group descendants' pipes promptly,
                    // but keep enforcing bounds if an external holder remains.
                    terminate(group);
                }
                Ok(None) => {}
                Err(_) => {
                    stopped = Some(FailureKind::Spawn);
                    terminate(group);
                    let _ = child.kill();
                    status = child.wait().ok();
                    break;
                }
            }
        }
        if status.is_some()
            && input_thread.is_finished()
            && stdout_thread.is_finished()
            && stderr_thread.is_finished()
        {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    // Descendants may hold the pipes after the immediate process exits.
    terminate(group);
    io_stop.store(true, Ordering::Release);
    let _ = input_thread.join();
    let mut stdout = stdout_thread.join().unwrap_or_default();
    let captured_stdout_bytes = stdout.len();
    if matches!(policy, ExecutePolicyV63::Strict) && !spec.sensitive_values.is_empty() {
        stdout = redact(&String::from_utf8_lossy(&stdout), &spec.sensitive_values).into_bytes();
    }
    let raw_stderr = stderr_thread.join().unwrap_or_default();
    let capture = CommandCapture {
        stdout_bytes: captured_stdout_bytes,
        stderr_bytes: raw_stderr.len(),
    };
    let stderr = redact(
        &sanitize_terminal(&String::from_utf8_lossy(&raw_stderr)),
        &spec.sensitive_values,
    )
    .into_bytes();
    if stopped.is_none() && size.load(Ordering::Relaxed) > limit {
        stopped = Some(FailureKind::OutputLimit);
    }
    let exit_code = status.and_then(|s| s.code()).unwrap_or(-1);
    if let Some(kind) = stopped {
        let unknown = matches!(spec.kind, CommandKind::Write { .. });
        let message = match kind {
            FailureKind::Timeout => "Command exceeded its deadline",
            FailureKind::Cancelled => "Command cancelled",
            FailureKind::OutputLimit => "Command exceeded its output limit",
            _ => "Command failed while running",
        };
        return Err(CommandFailure {
            kind,
            message: message.into(),
            exit_code: Some(exit_code),
            stdout: redact(
                &sanitize_terminal(&String::from_utf8_lossy(&stdout)),
                &spec.sensitive_values,
            )
            .into_bytes(),
            stderr,
            outcome_unknown: unknown,
        });
    }
    if exit_code != 0
        && !(matches!(
            policy,
            ExecutePolicyV63::NotificationConditionalGet | ExecutePolicyV63::WorkflowBranchGetV9
        ) && exit_code == 1)
    {
        return Err(CommandFailure {
            kind: FailureKind::Exit,
            message: format!(
                "Command exited with {exit_code}: {}",
                String::from_utf8_lossy(&stderr).trim()
            ),
            exit_code: Some(exit_code),
            stdout: redact(
                &sanitize_terminal(&String::from_utf8_lossy(&stdout)),
                &spec.sensitive_values,
            )
            .into_bytes(),
            stderr,
            outcome_unknown: matches!(spec.kind, CommandKind::Write { .. }),
        });
    }
    Ok(CommandOutput {
        stdout,
        stderr,
        exit_code,
        elapsed: start.elapsed(),
        capture,
    })
}
fn terminate(group: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(group as i32), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    let _ = group;
}

/// Remove ANSI/OSC controls, C0/C1 controls, and Unicode directional overrides.
pub fn sanitize_terminal(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') | Some('P') | Some('^') | Some('_') => {
                    while let Some(next) = chars.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if (!c.is_control() || c == '\n' || c == '\t')
            && !matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
        {
            output.push(c);
        }
    }
    output
}
pub fn redact(input: &str, secrets: &[String]) -> String {
    let mut result = input.to_owned();
    for secret in secrets {
        if !secret.is_empty() {
            result = result.replace(secret, "[REDACTED]");
        }
    }
    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"] {
        while let Some(start) = result.find(prefix) {
            let end = result[start..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(result.len(), |n| start + n);
            result.replace_range(start..end, "[REDACTED]");
        }
    }
    result
}

#[cfg(test)]
mod absolute_deadline_tests_v1 {
    use super::*;

    #[test]
    fn inline_comments_absolute_deadline_prevents_even_attempting_a_missing_program() {
        let mut spec = CommandSpec::new("ghtui-this-program-must-not-exist");
        spec.absolute_deadline_v1 = Some(Instant::now());
        let failure = CommandRunner::default()
            .run(spec, &CancellationToken::default())
            .unwrap_err();
        assert_eq!(failure.kind, FailureKind::Timeout);
        assert!(!failure.outcome_unknown);
    }

    #[test]
    fn inline_comments_absolute_deadline_terminates_an_actual_process_without_reset() {
        let mut spec = CommandSpec::new("/bin/sleep").args(["2"]).fresh_read();
        let started = Instant::now();
        spec.absolute_deadline_v1 = Some(started + Duration::from_millis(40));
        let failure = CommandRunner::default()
            .run(spec, &CancellationToken::default())
            .unwrap_err();
        assert_eq!(failure.kind, FailureKind::Timeout);
        assert!(!failure.outcome_unknown);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn inline_comments_absolute_deadline_bounds_write_lock_wait_without_launch() {
        let runner = CommandRunner::default();
        let lock = Arc::new(Mutex::new(ResourceState::default()));
        runner
            .state
            .lock()
            .unwrap()
            .writes
            .insert("original-inline-phase".into(), lock.clone());
        let _held = lock.lock().unwrap();
        let mut spec =
            CommandSpec::new("ghtui-this-program-must-not-exist").write("original-inline-phase");
        let started = Instant::now();
        spec.absolute_deadline_v1 = Some(started + Duration::from_millis(40));
        let failure = runner.run(spec, &CancellationToken::default()).unwrap_err();
        assert_eq!(failure.kind, FailureKind::Timeout);
        assert!(!failure.outcome_unknown);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn inline_comments_absolute_deadline_bounds_a_coalesced_subscriber_wait() {
        let runner = CommandRunner::default();
        let mut spec = CommandSpec::new("ghtui-this-program-must-not-exist");
        let started = Instant::now();
        spec.absolute_deadline_v1 = Some(started + Duration::from_millis(40));
        runner.state.lock().unwrap().reads.insert(
            ReadKey::from(&spec),
            Arc::new((Mutex::new(None), Condvar::new())),
        );
        let failure = runner.run(spec, &CancellationToken::default()).unwrap_err();
        assert_eq!(failure.kind, FailureKind::Timeout);
        assert!(!failure.outcome_unknown);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
