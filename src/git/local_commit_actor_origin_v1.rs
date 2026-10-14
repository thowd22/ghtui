//! Worker-observed local source continuity. This is NOT GitHub association,
//! access, a signature, or a restored approval. No public origin constructor.
use super::{
    local_commit_actor_v1::source_identity, CancellationToken, GitError, GitRepository, GitResult,
    LocalCommitActorLimits, LocalCommitSourceIdentity, OriginalLocalCommitActors,
};
use std::{sync::Arc, time::Instant};

pub struct OriginalLocalActorOriginV1 {
    repository: GitRepository,
    source: LocalCommitSourceIdentity,
}
impl std::fmt::Debug for OriginalLocalActorOriginV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalLocalActorOriginV1(<actual worker source; no remote authority>)")
    }
}
impl OriginalLocalActorOriginV1 {
    pub(crate) fn retained_graph_memory_bytes_v2(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.repository.root.capacity()
            + self.repository.git_dir.capacity()
            + self.repository.common_dir.capacity()
            + self.source.retained_memory_bytes_v2()
            - std::mem::size_of::<LocalCommitSourceIdentity>()
    }
    /// Called from the worker with the same discovered handle that actually
    /// supplies its Workspace. The reused observer bounds fields before copies.
    /// This captures filesystem metadata only: no extra Git/gh/SQL children.
    pub(crate) fn capture(repository: &GitRepository) -> GitResult<Arc<Self>> {
        check_cancel(&repository.cancellation)?;
        let source = source_identity(repository)?;
        check_cancel(&repository.cancellation)?;
        Ok(Arc::new(Self {
            repository: repository.clone(),
            source,
        }))
    }
    pub(crate) fn original_repository_v1(&self) -> &GitRepository {
        &self.repository
    }
    pub fn source(&self) -> &LocalCommitSourceIdentity {
        &self.source
    }
    /// Worker-only bounded original-source recheck. Input and draw instead
    /// compare the original immutable profile/owner handles without disk I/O.
    pub fn recheck(&self, cancel: &CancellationToken) -> GitResult<()> {
        check_cancel(cancel)?;
        if source_identity(&self.repository)? != self.source {
            return Err(GitError(
                "Original loaded local Git source/locator changed; no rebase".into(),
            ));
        }
        check_cancel(cancel)
    }
    /// Still inside the ORIGINAL loading worker, before its source is admitted.
    /// Later explicit Verify uses its own fresh request token instead.
    pub(crate) fn recheck_loading(&self) -> GitResult<()> {
        self.recheck(&self.repository.cancellation)
    }
    /// Explicit NEW local observation. A finished loading token is not reused:
    /// use the same genuine paths/handle with this request's fresh cancellation.
    /// Preflight time is included in the SAME absolute raw-token deadline.
    pub fn read_original(
        &self,
        sha: &str,
        limits: &LocalCommitActorLimits,
        cancel: &CancellationToken,
    ) -> GitResult<OriginalLocalCommitActors> {
        let deadline = Instant::now()
            .checked_add(limits.max_duration)
            .ok_or_else(|| GitError("Original local actor deadline unavailable".into()))?;
        self.read_original_under_deadline(sha, limits, deadline, cancel)
    }
    /// New explicit operation; raw Git children use its original producer,
    /// absolute queued deadline and measured ledger. Never attach a later lease.
    pub fn read_original_under_navigation_operation_v1(
        &self,
        sha: &str,
        limits: &LocalCommitActorLimits,
        operation: &Arc<crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1>,
    ) -> GitResult<OriginalLocalCommitActors> {
        operation.check().map_err(GitError)?;
        self.recheck(operation.cancellation())?;
        let token = OriginalLocalCommitActors::read_under_navigation_operation_v1(
            &self.repository,
            sha,
            limits,
            operation,
        )?;
        if token.source() != &self.source {
            return Err(GitError(
                "Raw related-PR token belongs to another Git source".into(),
            ));
        }
        self.recheck(operation.cancellation())?;
        operation.check().map_err(GitError)?;
        Ok(token)
    }
    pub(crate) fn read_original_under_deadline(
        &self,
        sha: &str,
        limits: &LocalCommitActorLimits,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> GitResult<OriginalLocalCommitActors> {
        if !original_saved_sha(sha)
            || limits.max_duration.is_zero()
            || limits.max_duration > std::time::Duration::from_secs(30)
            || !(1..=64 * 1024).contains(&limits.max_object_bytes)
        {
            return Err(GitError(
                "Original actor source needs full saved SHA and bounded read limits".into(),
            ));
        }
        let deadline = deadline.min(
            Instant::now()
                .checked_add(limits.max_duration)
                .ok_or_else(|| GitError("Original local actor deadline unavailable".into()))?,
        );
        check_deadline(deadline, cancel)?;
        self.recheck(cancel)?;
        check_deadline(deadline, cancel)?;
        // No discover()/rev-parse, ref resolution, lazy fetch or clone. Frozen
        // raw reader performs exactly its original TWO measured cat-file reads.
        let repository = self
            .repository
            .clone()
            .with_cancellation(cancel.clone())
            .strip_navigation_read_window_v1()?;
        let token = OriginalLocalCommitActors::read_under_deadline(
            &repository,
            sha,
            limits,
            deadline,
            cancel,
        )?;
        if token.source() != &self.source {
            return Err(GitError(
                "Raw actor token does not belong to original loaded local source".into(),
            ));
        }
        self.recheck(cancel)?;
        check_deadline(deadline, cancel)?;
        Ok(token)
    }
}
pub(crate) fn original_saved_sha(sha: &str) -> bool {
    matches!(sha.len(), 40 | 64)
        && sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && !sha.bytes().all(|byte| byte == b'0')
}
fn check_cancel(cancel: &CancellationToken) -> GitResult<()> {
    if cancel.is_cancelled() {
        return Err(GitError("Original local actor request cancelled".into()));
    }
    Ok(())
}
fn check_deadline(deadline: Instant, cancel: &CancellationToken) -> GitResult<()> {
    check_cancel(cancel)?;
    if Instant::now() >= deadline {
        return Err(GitError(
            "Original actor deadline expired before another read".into(),
        ));
    }
    Ok(())
}

// Git ignores Trace2 settings in local/-c config. Code contracts instead inject
// a scoped diagnostic env value into the SAME real command spec, without
// process-global env/PATH mutation. Entirely absent from production builds.
#[cfg(test)]
static FIXTURE_TRACES: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, std::path::PathBuf>>,
> = std::sync::OnceLock::new();
#[cfg(test)]
pub(crate) struct FixtureTraceGuard {
    root: std::path::PathBuf,
}
#[cfg(test)]
impl Drop for FixtureTraceGuard {
    fn drop(&mut self) {
        FIXTURE_TRACES
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&self.root);
    }
}
#[cfg(test)]
pub(crate) fn register_fixture_trace(
    root: &std::path::Path,
    trace: &std::path::Path,
) -> FixtureTraceGuard {
    use std::os::unix::ffi::OsStrExt;
    assert!(root.is_absolute() && root.as_os_str().as_bytes().len() <= 4096);
    assert!(trace.as_os_str().as_bytes().len() <= 4096 && trace.is_absolute());
    let mut traces = FIXTURE_TRACES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    assert!(
        !traces.contains_key(root),
        "Each fixture owns a distinct root"
    );
    assert!(traces.len() < 128, "Diagnostic registry is bounded");
    traces.insert(root.to_path_buf(), trace.to_path_buf());
    FixtureTraceGuard {
        root: root.to_path_buf(),
    }
}
#[cfg(test)]
pub(super) fn trace_spec_for_tests(
    spec: &mut crate::providers::runner::CommandSpec,
    directory: &std::path::Path,
) {
    if let Some(path) = FIXTURE_TRACES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(directory)
    {
        spec.env
            .insert("GIT_TRACE2_EVENT".into(), path.as_os_str().to_owned());
    }
}

#[cfg(test)]
#[path = "local_commit_actor_origin_v1_tests.rs"]
mod tests;
