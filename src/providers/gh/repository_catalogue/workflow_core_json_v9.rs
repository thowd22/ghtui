//! Closed W9 allocation policy installed BEFORE original Core allocations.
//! It restricts heap, never issues a source/principal/read/write permission.
use super::*;
use crate::providers::gh::actions::{
    WorkflowJsonCaptureV9, WorkflowJsonChargeV9, WorkflowJsonRoleV9,
};
use crate::scoped_actions_v1::{ScopedActionsPageV1, ScopedActionsSourceV1, ScopedActionsWorkV1};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct WorkflowCoreJsonV9 {
    ledger: Arc<WorkflowJsonLedgerV9>,
    original: Arc<RepositoryCatalogueRead>,
    entered: AtomicBool,
    deadline: Instant,
    _index: WorkflowJsonChargeV9,
    _models: WorkflowJsonChargeV9,
    _provider: WorkflowJsonChargeV9,
    _git_environment: Option<WorkflowJsonChargeV9>,
}
fn refused() -> GithubError {
    error(
        GithubErrorKind::Json,
        "Original W9 Core allocation/source boundary refused; private diagnostics omitted",
    )
}
impl WorkflowCoreJsonV9 {
    pub(crate) fn for_local_v9(
        provider: &GithubProvider,
        inputs: &crate::scoped_actions_workflow_edit_v9::WorkflowLocalCoreInputsV9<'_>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        let intent = inputs.intent();
        if !intent.original().original_writer_v6(provider)
            || !std::sync::Arc::ptr_eq(intent.source(), intent.page().source())
        {
            return Err(refused());
        }
        Self::original(
            provider,
            intent.source(),
            intent.page(),
            inputs.held_bytes(),
            cancel,
            deadline,
            true,
        )
    }
    fn original(
        provider: &GithubProvider,
        source: &ScopedActionsSourceV1,
        page: &ScopedActionsPageV1,
        held: usize,
        cancel: &CancellationToken,
        requested: Instant,
        needs_git: bool,
    ) -> GithubResult<Arc<Self>> {
        if provider.scoped_read_budget.is_some()
            || !std::ptr::eq(page.source().as_ref(), source)
            || source.original().scope().host != provider.host
        {
            return Err(refused());
        }
        let deadline = provider
            .scoped_read_window
            .map_or(requested, |(i, _)| i.min(requested));
        // Do not guess Arc dedup: these ACTUAL borrowed graphs are charged
        // conservatively in addition to captured Form/Preview material H.
        let graph = source
            .original()
            .workflow_owned_bytes_v9()
            .ok_or_else(refused)?
            .checked_add(
                source
                    .scope()
                    .workflow_owned_bytes_v9()
                    .ok_or_else(refused)?,
            )
            .and_then(|n| n.checked_add(page.retained_bytes()))
            .and_then(|n| {
                n.checked_add(match source.pr_origin_v2() {
                    Some(pr) => pr.workflow_owned_bytes_v9()?,
                    None => 0,
                })
            })
            .and_then(|n| {
                n.checked_add(match &source.history_pr_v5 {
                    Some(pr) => pr.workflow_owned_bytes_v9()?,
                    None => 0,
                })
            })
            .and_then(|n| {
                n.checked_add(std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>())
            })
            .ok_or_else(refused)?;
        let environment = provider
            .workflow_environment_v9
            .as_ref()
            .ok_or_else(refused)?;
        let held = held
            .checked_add(graph)
            .and_then(|n| n.checked_add(environment.retained_bytes_v9()))
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or_else(refused)?;
        let ledger = Arc::new(WorkflowJsonLedgerV9::before_core_v9(
            provider, held, cancel, deadline,
        )?);
        // SAME phase debit retained across every fixed Git child and the
        // closed prepaid continuation; never reset per child or at seal.
        let git_environment = if needs_git {
            Some(
                ledger.reserve_model_v9(
                    environment
                        .fixed_git_child_bound_v9()
                        .and_then(|n| n.checked_add(64 * 1024))
                        .ok_or_else(refused)?,
                )?,
            )
        } else {
            None
        };
        let n = source.original().source().repositories().len();
        if n > 100000 {
            return Err(refused());
        }
        // Exact slice/enumerate/map has exact size_hint n. std HashMap collect
        // reserves n once. This dominates pinned hashbrown's load-factor
        // buckets and <=16-byte group/padding; overflow refuses BEFORE collect.
        let buckets = n
            .checked_mul(2)
            .map(|n| n.max(4))
            .and_then(usize::checked_next_power_of_two)
            .ok_or_else(refused)?;
        let index = buckets
            .checked_mul(std::mem::size_of::<(u64, usize)>() + 1)
            .and_then(|n| {
                n.checked_add(64 + std::mem::size_of::<std::collections::HashMap<u64, usize>>())
            })
            .ok_or_else(refused)?;
        let index = ledger.reserve_model_v9(index)?;
        // Two bounded64KiB repo observations, one reference, identities,
        // parser-owned scalar models, failure/reference nodes and headers.
        // No original catalogue row/definition is omitted: graph above owns it.
        let models = ledger.reserve_model_v9(1024 * 1024)?;
        let env_node = 32
            + 16 * (2 * std::mem::size_of::<std::ffi::OsString>() + std::mem::size_of::<usize>());
        let provider_bytes = provider
            .env
            .iter()
            .try_fold(0usize, |n, (k, v)| {
                n.checked_add(k.capacity())?.checked_add(v.capacity())
            })
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| n.checked_add((provider.env.len() + 3).checked_mul(8 * env_node)?))
            .and_then(|n| n.checked_add(provider.program.capacity()))
            .and_then(|n| n.checked_add(provider.host.capacity()))
            .ok_or_else(refused)?;
        let provider_charge = ledger.reserve_model_v9(provider_bytes)?;
        Ok(Arc::new(Self {
            ledger,
            original: source.original().source().clone(),
            entered: AtomicBool::new(false),
            deadline,
            _index: index,
            _models: models,
            _git_environment: git_environment,
            _provider: provider_charge,
        }))
    }
    pub(crate) fn for_preview_v9(
        provider: &GithubProvider,
        request: &crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditRequestV9,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        use crate::scoped_actions_workflow_edit_ui_v9::{
            WorkflowEditPhaseV9 as Phase, WorkflowEditStepV9 as Step,
        };
        if request.phase() != Phase::Preview
            || !matches!(request.step(), Step::PublishBranch | Step::CreatePr)
            || !request
                .opened()
                .intent()
                .original()
                .original_writer_v6(provider)
        {
            return Err(refused());
        }
        let intent = request.opened().intent();
        Self::original(
            provider,
            intent.source(),
            intent.page(),
            request.owned_bytes(),
            cancel,
            deadline,
            true,
        )
    }
    pub(crate) fn for_confirm_v9(
        provider: &GithubProvider,
        request: &crate::scoped_actions_workflow_edit_v9::WorkflowRemoteConfirmV9,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        if !request.cancellation().same_instance(cancel) || deadline > request.deadline() {
            return Err(refused());
        }
        let intent = request
            .preview()
            .selection()
            .committed()
            .staged()
            .saved()
            .opened()
            .intent();
        if !intent.original().original_writer_v6(provider) {
            return Err(refused());
        }
        Self::original(
            provider,
            intent.source(),
            intent.page(),
            request.held_material_bytes_v9(),
            cancel,
            deadline,
            true,
        )
    }
    pub(crate) fn for_standalone_preview_v9(
        provider: &GithubProvider,
        committed: &crate::scoped_actions_workflow_edit_v9::WorkflowCommittedLocalV9,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        let intent = committed.staged().saved().opened().intent();
        if !intent.original().original_writer_v6(provider) {
            return Err(refused());
        }
        Self::original(
            provider,
            intent.source(),
            intent.page(),
            committed.retained_bytes_v9().ok_or_else(refused)?,
            cancel,
            deadline,
            true,
        )
    }
    pub(crate) fn for_history_v9(
        provider: &GithubProvider,
        work: &ScopedActionsWorkV1,
    ) -> GithubResult<Arc<Self>> {
        work.check().map_err(|_| refused())?;
        let request = work
            .journal_v5
            .as_ref()
            .filter(|r| r.matches_page(&work.page))
            .ok_or_else(refused)?;
        let held = request.held_workflow_bytes_v9();
        if held == 0 {
            return Err(refused());
        }
        Self::original(
            provider,
            &work.source,
            &work.page,
            held,
            work.cancellation(),
            work.deadline(),
            false,
        )
    }
    pub(super) fn enter(
        &self,
        provider: &GithubProvider,
        original: &RepositoryCatalogueRead,
        limits: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if !std::ptr::eq(self.original.as_ref(), original)
            || limits.max_repository_reads != 2
            || limits.transport.max_document_bytes > 64 * 1024
            || limits.transport.max_response_bytes > 2 * 1024 * 1024
            || limits.transport.max_duration > Duration::from_secs(30)
            || self.deadline > provider.scoped_read_window.ok_or_else(refused)?.0
        {
            return Err(refused());
        }
        self.ledger
            .capture_v9(provider, WorkflowJsonRoleV9::CoreVersion, cancel)?;
        self.entered
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| refused())?;
        Ok(())
    }
    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(super) fn check(&self, cancel: &CancellationToken) -> GithubResult<()> {
        if cancel.is_cancelled() || Instant::now() >= self.deadline {
            return Err(refused());
        }
        self.ledger.remaining_v9().map(|_| ())
    }
    pub(super) fn capture(
        &self,
        provider: &GithubProvider,
        role: WorkflowJsonRoleV9,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowJsonCaptureV9> {
        self.check(cancel)?;
        self.ledger.capture_v9(provider, role, cancel)
    }
    pub(crate) fn bind_measured_v9(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<WorkflowJsonLedgerV9>> {
        self.ledger.bind_measured_v9(provider, cancel)?;
        Ok(self.ledger.clone())
    }
}
