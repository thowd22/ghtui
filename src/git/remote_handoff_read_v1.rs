//! Source-only child of installed-Git provider. The actual shared runner spec
//! retains no-network/optional-lock policies and original measured READ pool.
use super::*;
use crate::providers::remote_git_read_window_v1::RemoteGitReadWindowV1;

pub(super) struct GitReadPolicyV1<'a> {
    pub timeout: Duration,
    pub output_limit: usize,
    pub window: Option<&'a RemoteGitReadWindowV1>,
    pub runner: Option<&'a CommandRunner>,
}

impl GitRepository {
    /// The producer survives a completed operation. Its pool and clock do not.
    pub(crate) fn actual_read_runner_v1(&self) -> &CommandRunner {
        self.navigation_read_window
            .as_ref()
            .map(RemoteGitReadWindowV1::actual_runner_v1)
            .or(self.retained_read_runner_v1.as_ref())
            .unwrap_or_else(|| RUNNER.get_or_init(CommandRunner::default))
    }
    pub(crate) fn discover_with_remote_read_window_v1(
        directory: &Path,
        cancel: CancellationToken,
        window: RemoteGitReadWindowV1,
    ) -> GitResult<Option<Self>> {
        Self::discover_with_read_policy_v1(directory, cancel, Some(window))
    }
    /// NEW explicit local request only. Keep genuine paths/identity; loading
    /// cancellation is not reset by recheck_loading or the old read itself.
    pub(crate) fn strip_navigation_read_window_v1(mut self) -> GitResult<Self> {
        if self
            .navigation_read_window
            .as_ref()
            .is_some_and(|window| !window.bounds_completed_v1().unwrap_or(false))
        {
            return Err(GitError(
                "A live/unknown original navigation window cannot be stripped or revived".into(),
            ));
        }
        if let Some(window) = &self.navigation_read_window {
            self.retained_read_runner_v1 = Some(window.actual_runner_v1().clone());
        }
        self.navigation_read_window = None;
        Ok(self)
    }
}
pub(super) fn run_git_with_window_v1<I, S>(
    directory: &Path,
    args: I,
    stdin: Option<&[u8]>,
    write: bool,
    cancellation: &CancellationToken,
    window: Option<&RemoteGitReadWindowV1>,
    runner: Option<&CommandRunner>,
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
            timeout: COMMAND_TIMEOUT,
            output_limit: MAX_OUTPUT,
            window,
            runner,
        },
    )
}

#[cfg(test)]
#[path = "remote_handoff_read_v1_tests.rs"]
mod tests;
