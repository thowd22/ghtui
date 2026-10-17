//! Closed workflow-edit transport on the SAME original prepaid cursor.
//! This is bounds lineage, never a Git/source/account/publication permit.
use super::*;
#[path = "workflow_cleanup_v9.rs"]
mod workflow_cleanup_v9;
use crate::git::workflow_edit_v9::{
    WorkflowGitCommandV9, WorkflowGitOutputV9, WorkflowGitWriteCommandV9,
    WorkflowRemoteGitCommandV9, WorkflowWorktreeReadCommandV9, WorkflowWorktreeWriteCommandV9,
};
pub(crate) use workflow_cleanup_v9::WorkflowCleanupSealV9;

const KIB: usize = 1024;
pub(crate) const WORKFLOW_EDIT_READS_V9: usize = 42;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkflowEditPhaseV9 {
    Bind,
    Save,
    Worktree,
    Stage,
    Commit,
    PublishBranch,
    CreatePr,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkflowGitRoleV9 {
    Write,
    Discover(u8),
    Status(u8),
    Fingerprint(u8),
    ResolveHead,
    CommitSize,
    CommitObject,
    Snapshot(u8),
    WorktreeList,
    ExactPushedRef,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoleV9 {
    Git(WorkflowGitRoleV9),
    PrCreate,
    Branch,
    PrReadback,
    Principal,
}
#[derive(Clone, Copy)]
struct SlotV9 {
    role: RoleV9,
    cap: usize,
}
impl WorkflowEditPhaseV9 {
    fn slots(self) -> Vec<SlotV9> {
        use WorkflowGitRoleV9 as G;
        let mut out = Vec::with_capacity(18);
        let mut git = |role, cap| {
            out.push(SlotV9 {
                role: RoleV9::Git(role),
                cap: cap * KIB,
            })
        };
        match self {
            Self::Bind => return out,
            Self::Save => {}
            Self::Worktree => {
                git(G::Write, 256);
                for i in 0..4 {
                    git(G::Discover(i), 4);
                }
                for (i, cap) in [64, 4, 4, 64].into_iter().enumerate() {
                    git(G::Status(i as u8), cap);
                }
                git(G::ResolveHead, 4);
                git(G::WorktreeList, 64);
            }
            Self::Stage => {
                git(G::Write, 256);
                for (i, cap) in [64, 4, 4, 64].into_iter().enumerate() {
                    git(G::Status(i as u8), cap);
                }
                for (i, cap) in [64, 64, 64, 64, 256].into_iter().enumerate() {
                    git(G::Fingerprint(i as u8), cap);
                }
            }
            Self::Commit => {
                git(G::Write, 256);
                git(G::ResolveHead, 4);
                git(G::CommitSize, 4);
                git(G::CommitObject, 64);
                for (i, cap) in [4, 4, 256, 64].into_iter().enumerate() {
                    git(G::Snapshot(i as u8), cap);
                }
                for (i, cap) in [64, 4, 4, 64].into_iter().enumerate() {
                    git(G::Status(i as u8), cap);
                }
                for (i, cap) in [64, 64, 64, 64, 256].into_iter().enumerate() {
                    git(G::Fingerprint(i as u8), cap);
                }
            }
            Self::PublishBranch => {
                git(G::Write, 256);
                git(G::ExactPushedRef, 8);
                out.push(SlotV9 {
                    role: RoleV9::Branch,
                    cap: 16 * KIB,
                });
            }
            Self::CreatePr => {
                out.push(SlotV9 {
                    role: RoleV9::PrCreate,
                    cap: 256 * KIB,
                });
                out.push(SlotV9 {
                    role: RoleV9::PrReadback,
                    cap: 128 * KIB,
                });
            }
        }
        out.push(SlotV9 {
            role: RoleV9::Principal,
            cap: 68 * KIB,
        });
        out
    }
    pub(crate) fn post_reads(self) -> usize {
        self.slots()
            .iter()
            .filter(|slot| {
                !matches!(
                    slot.role,
                    RoleV9::Git(WorkflowGitRoleV9::Write) | RoleV9::PrCreate
                )
            })
            .count()
    }
    pub(crate) fn pre_reads(self) -> usize {
        WORKFLOW_EDIT_READS_V9 - 6 - self.post_reads()
    }
}

/// No Clone, serde or public constructor. `inner.next` is the ONLY cursor.
pub(crate) struct WorkflowPostCaptureV9 {
    inner: PostWriteCaptureAllowanceV2,
    original: Arc<ScopedActionsWorkflowV2>,
    phase: WorkflowEditPhaseV9,
    slots: Box<[SlotV9]>,
}
impl std::fmt::Debug for WorkflowPostCaptureV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowPostCaptureV9(<same fixed original cursor; no permission>)")
    }
}
fn refused() -> GithubError {
    error(
        GithubErrorKind::InvalidTarget,
        "Original workflow phase/role/issuer/cursor unavailable; no renewed command",
    )
}
impl WorkflowPostCaptureV9 {
    pub(crate) fn remote_git_v9(
        &mut self,
        provider: &GithubProvider,
        command: WorkflowRemoteGitCommandV9<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if self.phase != WorkflowEditPhaseV9::PublishBranch {
            return Err(refused());
        }
        command.validate_original(&provider.runner, &self.original, cancel, self.deadline())?;
        let bounded = self.bounded(provider, RoleV9::Git(command.role()), cancel)?;
        let (_, cap) = bounded.scoped_read_window.ok_or_else(refused)?;
        let result = command.execute_bounded(&bounded.runner, cap, cancel);
        self.inner.check_issuer(provider, cancel)?;
        result
    }
    pub(crate) fn observe_pushed_branch_v9(
        &mut self,
        provider: &GithubProvider,
        refs: &Arc<crate::providers::gh::WorkflowRemoteRefsV9>,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::WorkflowBranchObservedV9> {
        refs.validate_post_v9(
            provider,
            cancel,
            self.deadline(),
            WorkflowEditPhaseV9::PublishBranch,
        )?;
        if self.phase != WorkflowEditPhaseV9::PublishBranch
            || !Arc::ptr_eq(&self.original, refs.original_v9())
        {
            return Err(refused());
        }
        let bounded = self.bounded(provider, RoleV9::Branch, cancel)?;
        let (sha, kind) = refs.read_paid_head_v9(&bounded, heap, cancel)?;
        self.inner.check_issuer(provider, cancel)?;
        if kind != "commit" || sha != refs.committed().commit().head() {
            return Err(refused());
        }
        Ok(crate::providers::gh::WorkflowBranchObservedV9::after_paid_ref_v9(refs))
    }
    pub(crate) fn create_workflow_pr_v9(
        &mut self,
        provider: &GithubProvider,
        payload: &Arc<crate::providers::gh::WorkflowPrPayloadV9>,
        release: &crate::scoped_actions_workflow_edit_v9::WorkflowRemoteReleaseV9<'_>,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::WorkflowPrCreatedV9> {
        let refs = payload.refs();
        release.check_refs_v9(refs)?;
        refs.validate_post_v9(
            provider,
            cancel,
            self.deadline(),
            WorkflowEditPhaseV9::CreatePr,
        )?;
        if self.phase != WorkflowEditPhaseV9::CreatePr
            || !Arc::ptr_eq(&self.original, refs.original_v9())
            || payload.payload_v9().len() > 1024 * 1024
        {
            return Err(refused());
        }
        let bounded = self.bounded(provider, RoleV9::PrCreate, cancel)?;
        let capture = heap.capture_v9(&bounded, WorkflowJsonRoleV9::CreatedPr201, cancel)?;
        // Payload Arc is already retained by the model. Charge ALL additional
        // stdin copies BEFORE to_vec: supplied spec, Write runner spec clone,
        // and stdin-thread clone. Their raw bytes are not covered by32*output.
        let _input = heap.reserve_model_v9(
            payload
                .payload_v9()
                .len()
                .checked_mul(3)
                .ok_or_else(refused)?,
        )?;
        let resource = format!(
            "{}/repository-id/{}/actions/workflow/{}",
            provider.host,
            refs.repositories().base().id(),
            self.original.model().id
        );
        let mut spec = bounded.spec(vec![
            "api".into(),
            payload.endpoint_v9(),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "POST".into(),
            "--include".into(),
            "--input".into(),
            "-".into(),
        ]);
        spec.stdin = payload.payload_v9().to_vec();
        spec = spec.write(bounded.canonical_resource(&resource));
        capture.check_spec_v9(&mut spec)?;
        let result = (|| {
            let output = bounded
                .runner
                .run(spec, cancel)
                .map_err(|_| refused())?
                .stdout;
            self.inner.check_issuer(provider, cancel)?;
            let envelope = super::pinned_http_response_v5::response(
                &output,
                super::pinned_http_response_v5::ProfileV5::WorkflowPrCreated201V9,
            )?;
            let body_start = output
                .len()
                .checked_sub(envelope.body.len())
                .ok_or_else(refused)?;
            let length = output.len();
            let created = capture.decode_owned_v9(output, body_start..length)?;
            if !created.value().is_object() || created.value().get("errors").is_some() {
                return Err(refused());
            }
            let read = self.bounded(provider, RoleV9::PrReadback, cancel)?;
            // Actual raw201 output AND created tree remain live here. The
            // SAME ledger sees both when canonical GET reserves its capture.
            let current = payload.read_created_pr_v9(&read, &created, heap, cancel)?;
            self.inner.check_issuer(provider, cancel)?;
            payload.bind_created_v9(&read, created.value(), current.value())
        })();
        if result.is_err() {
            provider.runner.mark_ambiguous(&resource);
        }
        result
    }
    pub(crate) fn worktree_write_v9(
        &mut self,
        provider: &GithubProvider,
        command: WorkflowWorktreeWriteCommandV9<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if self.phase != WorkflowEditPhaseV9::Worktree {
            return Err(refused());
        }
        command.validate_original(&provider.runner, &self.original, cancel, self.deadline())?;
        let bounded = self.bounded(provider, RoleV9::Git(WorkflowGitRoleV9::Write), cancel)?;
        let (_, cap) = bounded.scoped_read_window.ok_or_else(refused)?;
        let result = command.execute_bounded(&bounded.runner, cap, self.deadline(), cancel);
        self.inner.check_issuer(provider, cancel)?;
        result
    }
    pub(crate) fn worktree_git_v9(
        &mut self,
        provider: &GithubProvider,
        command: WorkflowWorktreeReadCommandV9,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowGitOutputV9> {
        if self.phase != WorkflowEditPhaseV9::Worktree {
            return Err(refused());
        }
        command.validate_original(&provider.runner, &self.original, cancel, self.deadline())?;
        let bounded = self.bounded(provider, RoleV9::Git(command.role()), cancel)?;
        let (_, cap) = bounded.scoped_read_window.ok_or_else(refused)?;
        let result = command.execute_bounded(&bounded.runner, cap, self.deadline(), cancel);
        self.inner.check_issuer(provider, cancel)?;
        result
    }
    pub(crate) fn phase(&self) -> WorkflowEditPhaseV9 {
        self.phase
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.inner.deadline()
    }
    pub(crate) fn reserved_bytes(&self) -> usize {
        self.inner.reserved_bytes()
    }
    pub(crate) fn consumed(&self) -> usize {
        self.inner.consumed_requests()
    }
    pub(crate) fn validate_prewrite(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if self.inner.consumed_requests() != 0 || !self.original.original_writer_v6(provider) {
            return Err(refused());
        }
        self.inner.validate_prewrite(provider, cancel)
    }
    fn bounded(
        &mut self,
        provider: &GithubProvider,
        role: RoleV9,
        cancel: &CancellationToken,
    ) -> GithubResult<GithubProvider> {
        if self
            .slots
            .get(self.inner.consumed_requests())
            .is_none_or(|slot| slot.role != role)
            || !self.original.original_writer_v6(provider)
        {
            return Err(refused());
        }
        // This increments the ORIGINAL cursor before any child. The old pool
        // is CLOSED, not reused; the returned clone has no measured budget.
        self.inner.spend_ordinary_slot_v2(provider, cancel)
    }
    pub(crate) fn git(
        &mut self,
        provider: &GithubProvider,
        command: WorkflowGitCommandV9,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowGitOutputV9> {
        command.validate_original(
            &provider.runner,
            &self.original,
            self.phase,
            cancel,
            self.deadline(),
        )?;
        let bounded = self.bounded(provider, RoleV9::Git(command.role()), cancel)?;
        let (_, cap) = bounded.scoped_read_window.ok_or_else(refused)?;
        let output = command.execute_bounded(&bounded.runner, cap, self.deadline(), cancel)?;
        self.inner.check_issuer(provider, cancel)?;
        Ok(output)
    }
    pub(crate) fn local_write_v9(
        &mut self,
        provider: &GithubProvider,
        command: WorkflowGitWriteCommandV9<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        command.validate_original(
            &provider.runner,
            &self.original,
            self.phase,
            cancel,
            self.deadline(),
        )?;
        let bounded = self.bounded(provider, RoleV9::Git(WorkflowGitRoleV9::Write), cancel)?;
        let (_, cap) = bounded.scoped_read_window.ok_or_else(refused)?;
        let result = command.execute_bounded(&bounded.runner, cap, self.deadline(), cancel);
        self.inner.check_issuer(provider, cancel)?;
        result
    }
    #[cfg(test)]
    pub(crate) fn final_principal(
        &mut self,
        provider: &GithubProvider,
        expected: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<HostIdentity> {
        self.final_principal_inner_v9(provider, expected, None, cancel)
    }
    pub(crate) fn final_principal_with_heap_v9(
        &mut self,
        provider: &GithubProvider,
        expected: &HostIdentity,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<HostIdentity> {
        self.final_principal_inner_v9(provider, expected, Some(heap), cancel)
    }
    fn final_principal_inner_v9(
        &mut self,
        provider: &GithubProvider,
        expected: &HostIdentity,
        heap: Option<&WorkflowJsonLedgerV9>,
        cancel: &CancellationToken,
    ) -> GithubResult<HostIdentity> {
        let bounded = self.bounded(provider, RoleV9::Principal, cancel)?;
        let legacy;
        let guarded;
        let raw = if let Some(heap) = heap {
            let capture =
                heap.capture_v9(&bounded, WorkflowJsonRoleV9::PaidFinalPrincipal, cancel)?;
            let mut spec = bounded.spec(vec![
                "api".into(),
                "user".into(),
                "--hostname".into(),
                bounded.host.clone(),
                "--method".into(),
                "GET".into(),
            ]);
            spec.original_read_cancellation = Some(cancel.clone());
            spec.output_limit = spec.output_limit.min(capture.cap_v9());
            capture.check_spec_v9(&mut spec)?;
            let output = bounded
                .runner
                .run(spec, cancel)
                .map_err(|_| refused())?
                .stdout;
            let length = output.len();
            guarded = capture.decode_owned_v9(output, 0..length)?;
            guarded.value()
        } else {
            legacy = bounded.api("user", "GET", None, None, false, cancel)?;
            &legacy
        };
        let uid = raw
            .get("id")
            .and_then(Value::as_u64)
            .filter(|id| (1..=i64::MAX as u64).contains(id))
            .ok_or_else(refused)?;
        let login = raw
            .get("login")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control))
            .ok_or_else(refused)?;
        if expected.host != provider.host
            || expected.account_id != Some(uid)
            || self.inner.consumed_requests() != self.slots.len()
        {
            return Err(refused());
        }
        let mut current = expected.clone();
        current.login = login.into();
        self.inner.check_issuer(provider, cancel)?;
        Ok(current)
    }
}
impl OriginalRepositoryAccessCoreV13<'_> {
    pub(in crate::providers::gh::repository_catalogue::current_access_v1) fn reserve_workflow_edit_transport_v9(
        &mut self,
        original: &Arc<ScopedActionsWorkflowV2>,
        phase: WorkflowEditPhaseV9,
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<(MeasuredExternalReadWindowV1, Option<WorkflowPostCaptureV9>)> {
        if self.limits.transport.max_response_bytes > 2 * 1024 * 1024
            || !self
                .verified
                .iter()
                .rev()
                .find(|row| row.original().id == original.repository().id())
                .is_some_and(|row| original.matches_dispatch_pool_v7(self.provider, row))
        {
            return Err(refused());
        }
        let slots = phase.slots();
        let post = if slots.is_empty() {
            None
        } else {
            let caps = slots.iter().map(|slot| slot.cap).collect::<Vec<_>>();
            let inner =
                self.reserve_postwrite_capture_v2(&caps, remaining_repository_reads, cancel)?;
            Some(WorkflowPostCaptureV9 {
                inner,
                original: original.clone(),
                phase,
                slots: slots.into(),
            })
        };
        let window = self.reserve_workflow_edit_measured_v9(
            original,
            phase,
            remaining_repository_reads,
            cancel,
        )?;
        Ok((window, post))
    }
}
impl RepositoryCatalogueAccessSession<'_> {
    pub(crate) fn reserve_workflow_edit_transport_v9(
        &mut self,
        original: &Arc<ScopedActionsWorkflowV2>,
        phase: WorkflowEditPhaseV9,
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<(MeasuredExternalReadWindowV1, Option<WorkflowPostCaptureV9>)> {
        self.core.reserve_workflow_edit_transport_v9(
            original,
            phase,
            remaining_repository_reads,
            cancel,
        )
    }
}
