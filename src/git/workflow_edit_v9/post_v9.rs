//! Fixed ordered post-write facts from the SAME CLOSED prepaid cursor.
//! No revived measured window, generic read closure or second Git parser.
use super::*;
impl WorkflowLocalBindingV9 {
    pub(crate) fn post_commit_v9(
        self: &Arc<Self>,
        provider: &GithubProvider,
        capture: &mut WorkflowPostCaptureV9,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowObservedCommitV9> {
        if capture.phase() != WorkflowEditPhaseV9::Commit {
            return Err(post_unavailable());
        }
        let head =
            self.post_resolve_head(provider, WorkflowEditPhaseV9::Commit, capture, cancel)?;
        let base = &self.intent.definition().commit_sha;
        if head == *base {
            return Err(post_unavailable());
        }
        let mut read = |kind| {
            let command = self.read_command(kind, WorkflowEditPhaseV9::Commit, capture, cancel);
            capture.git(provider, command, cancel)?.bytes()
        };
        let size = read(ReadKindV9::CommitSize(head.clone()))?;
        let size = std::str::from_utf8(trim_line(&size))
            .ok()
            .and_then(|size| size.parse::<usize>().ok())
            .filter(|size| (1..=64 * 1024).contains(size))
            .ok_or_else(post_unavailable)?;
        let raw_commit = read(ReadKindV9::CommitObject(head.clone()))?;
        if raw_commit.len() != size
            || original_parent_ids_v9(&raw_commit)
                .map_err(|_| post_unavailable())?
                .as_slice()
                != [base.clone()]
        {
            return Err(post_unavailable());
        }
        let mut snapshot = |part| {
            read(ReadKindV9::Snapshot {
                part,
                base: base.clone(),
                head: head.clone(),
            })
        };
        if trim_line(&snapshot(0)?) != base.as_bytes()
            || trim_line(&snapshot(1)?) != head.as_bytes()
        {
            return Err(post_unavailable());
        }
        let patch = snapshot(2)?;
        // Count borrowed rows before the SAME canonical parser can allocate
        // per-line structs. Oversized post-hook diffs are inspection-required,
        // never partial success or permission to resend the actual Commit.
        if patch.split(|byte| *byte == b'\n').count() > 65_500 {
            return Err(post_unavailable());
        }
        let mut diff = Diff::parse(&patch).map_err(|_| post_unavailable())?;
        let paths = parse_diff_paths(&snapshot(3)?).map_err(|_| post_unavailable())?;
        if diff.files.len() != paths.len() || diff.files.len() != 1 {
            return Err(post_unavailable());
        }
        let path = self.intent.definition().workflow.path.as_bytes();
        for (file, (old, new)) in diff.files.iter_mut().zip(paths) {
            if old.as_deref() != Some(path)
                || new.as_deref() != Some(path)
                || file.metadata.iter().any(|line| {
                    line.starts_with(b"old mode ")
                        || line.starts_with(b"new mode ")
                        || line.starts_with(b"new file mode ")
                        || line.starts_with(b"deleted file mode ")
                })
            {
                return Err(post_unavailable());
            }
            file.old_path = old;
            file.new_path = new;
        }
        diff.base = Some(base.clone());
        diff.head = Some(head.clone());
        let comparison = ComparisonResult {
            comparison: Comparison::Snapshots {
                base: base.clone(),
                head: head.clone(),
            },
            description: "Compare two saved versions".into(),
            base: Some(base.clone()),
            head: Some(head.clone()),
            common_ancestor: None,
            diff,
            patch,
            observed: None,
        };
        self.check_origin_v9(cancel)
            .map_err(|_| post_unavailable())?;
        if cancel.is_cancelled() || Instant::now() >= capture.deadline() {
            return Err(post_unavailable());
        }
        Ok(WorkflowObservedCommitV9 {
            head,
            raw_commit,
            comparison,
        })
    }
    fn post_repository_v9(&self, cancel: &CancellationToken) -> GitResult<GitRepository> {
        self.check_origin_v9(cancel)?;
        self.origin
            .original_repository_v1()
            .clone()
            .strip_navigation_read_window_v1()
            .map(|repository| repository.with_cancellation(cancel.clone()))
    }
    pub(crate) fn post_status_v9(
        self: &Arc<Self>,
        provider: &GithubProvider,
        capture: &mut WorkflowPostCaptureV9,
        cancel: &CancellationToken,
    ) -> GithubResult<RepoStatus> {
        if !matches!(
            capture.phase(),
            WorkflowEditPhaseV9::Worktree
                | WorkflowEditPhaseV9::Stage
                | WorkflowEditPhaseV9::Commit
        ) {
            return Err(post_unavailable());
        }
        let phase = capture.phase();
        let mut read = |part| {
            let command = self.read_command(ReadKindV9::Status(part), phase, capture, cancel);
            capture.git(provider, command, cancel)?.bytes()
        };
        let status = read(0)?;
        let mut status = parse_status(&status).map_err(|_| post_unavailable())?;
        let repository = self
            .post_repository_v9(cancel)
            .map_err(|_| post_unavailable())?;
        status.operation = repository.sequencer();
        status.shallow = text(trim_line(&read(1)?)) == "true";
        status.sparse = read(2)? == b"true\n";
        let configured =
            std::env::var_os("GIT_REPLACE_REF_BASE").unwrap_or_else(|| "refs/replace".into());
        let prefix = replacement_prefix(&configured);
        let references =
            GitRepository::references_from_bytes_v9(&read(3)?).map_err(|_| post_unavailable())?;
        (status.replacement_refs, status.grafted) =
            repository.history_override_flags_v9(&references, &prefix);
        self.check_origin_v9(cancel)
            .map_err(|_| post_unavailable())?;
        if cancel.is_cancelled() || Instant::now() >= capture.deadline() {
            return Err(post_unavailable());
        }
        Ok(status)
    }
    pub(crate) fn post_fingerprint_v9(
        self: &Arc<Self>,
        provider: &GithubProvider,
        capture: &mut WorkflowPostCaptureV9,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkspaceFingerprint> {
        if !matches!(
            capture.phase(),
            WorkflowEditPhaseV9::Stage | WorkflowEditPhaseV9::Commit
        ) {
            return Err(post_unavailable());
        }
        let repository = self
            .post_repository_v9(cancel)
            .map_err(|_| post_unavailable())?;
        let phase = capture.phase();
        let mut read = |part| {
            let command = self.read_command(ReadKindV9::Fingerprint(part), phase, capture, cancel);
            capture.git(provider, command, cancel)?.bytes()
        };
        let mut hash = DefaultHasher::new();
        let status = read(0)?;
        status.hash(&mut hash);
        read(1)?.hash(&mut hash);
        read(2)?.hash(&mut hash);
        let config = read(3)?;
        repository
            .hash_remote_config_v9(&config, &mut hash)
            .map_err(|_| post_unavailable())?;
        read(4)?.hash(&mut hash);
        if config != self.config {
            return Err(post_unavailable());
        }
        let (fingerprint, _) = repository
            .finish_fingerprint_v9(256 * 1024, hash, &status, config, Some(capture.deadline()))
            .map_err(|_| post_unavailable())?;
        self.check_origin_v9(cancel)
            .map_err(|_| post_unavailable())?;
        Ok(fingerprint)
    }
}

/// Facts from the actual post-Commit child sequence. No saved ID constructor,
/// remote association, source factory, approval or public raw transport.
pub(crate) struct WorkflowObservedCommitV9 {
    head: String,
    raw_commit: Vec<u8>,
    comparison: ComparisonResult,
}
impl WorkflowObservedCommitV9 {
    pub(crate) fn retained_bytes_v9(&self) -> Option<usize> {
        let comparison = &self.comparison;
        let endpoint_bytes = match &comparison.comparison {
            Comparison::Snapshots { base, head } | Comparison::BranchChanges { base, head } => {
                base.capacity().checked_add(head.capacity())?
            }
            Comparison::CommitChanges { commit, .. } => commit.capacity(),
            _ => 0,
        };
        let mut n = std::mem::size_of::<Self>()
            .checked_add(self.head.capacity())?
            .checked_add(self.raw_commit.capacity())?
            .checked_add(endpoint_bytes)?
            .checked_add(comparison.description.capacity())?
            .checked_add(comparison.patch.capacity())?;
        for endpoint in [
            &comparison.base,
            &comparison.head,
            &comparison.common_ancestor,
            &comparison.diff.base,
            &comparison.diff.head,
        ] {
            n = n.checked_add(endpoint.as_ref().map_or(0, String::capacity))?;
        }
        n = n.checked_add(
            comparison
                .diff
                .files
                .capacity()
                .checked_mul(std::mem::size_of::<crate::diff::FileDiff>())?,
        )?;
        for file in &comparison.diff.files {
            for path in [&file.old_path, &file.new_path] {
                n = n.checked_add(path.as_ref().map_or(0, Vec::capacity))?;
            }
            n = n.checked_add(
                file.metadata
                    .capacity()
                    .checked_mul(std::mem::size_of::<Vec<u8>>())?,
            )?;
            for row in &file.metadata {
                n = n.checked_add(row.capacity())?;
            }
            n = n.checked_add(
                file.hunks
                    .capacity()
                    .checked_mul(std::mem::size_of::<crate::diff::Hunk>())?,
            )?;
            for hunk in &file.hunks {
                n = n.checked_add(hunk.heading.capacity())?.checked_add(
                    hunk.lines
                        .capacity()
                        .checked_mul(std::mem::size_of::<crate::diff::DiffLine>())?,
                )?;
                for line in &hunk.lines {
                    n = n.checked_add(line.text.capacity())?;
                }
            }
        }
        Some(n)
    }
    pub(crate) fn head(&self) -> &str {
        &self.head
    }
    pub(crate) fn comparison(&self) -> &ComparisonResult {
        &self.comparison
    }
}
impl fmt::Debug for WorkflowObservedCommitV9 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "WorkflowObservedCommitV9(<actual D and canonical C..D; private bytes omitted>)",
        )
    }
}
