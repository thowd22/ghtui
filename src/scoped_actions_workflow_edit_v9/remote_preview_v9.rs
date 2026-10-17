//! A genuine bounded read bracket prepares remote Preview. Closed material
//! never preserves the old measured pool/clock as a later Confirm permit.
use super::*;
use crate::{
    git::workflow_edit_v9::WorkflowRemoteGitObservationV9,
    prepared_form_text_v1::PreparedFormTextV1,
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_publication_v3 as shared,
};
use std::sync::atomic::{AtomicBool, Ordering};
struct RemotePreviewOwnerV9<'a> {
    budget: WorkflowUiBudgetV9,
    branch: Option<Arc<WorkflowPublishedBranchV9>>,
    request: Option<&'a crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditRequestV9>,
}

pub(crate) struct WorkflowRemotePreviewV9 {
    selection: Arc<WorkflowRemoteSelectionV9>,
    phase: WorkflowEditPhaseV9,
    repositories: Arc<WorkflowRemoteRepositoriesV9>,
    refs: Arc<WorkflowRemoteRefsV9>,
    git: Arc<WorkflowRemoteGitObservationV9>,
    _retained_payload: Option<Arc<WorkflowPrPayloadV9>>,
    lines: Vec<String>,
    display: Arc<PreparedFormTextV1>,
    _retained_identity: HostIdentity,
    retained: usize,
    attempted: AtomicBool,
    branch: Option<Arc<WorkflowPublishedBranchV9>>,
}
impl std::fmt::Debug for WorkflowRemotePreviewV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowRemotePreviewV9(<closed actual C/D/member/ref/transport preview; no live permit>)")
    }
}
impl WorkflowRemotePreviewV9 {
    pub(crate) fn selection(&self) -> &Arc<WorkflowRemoteSelectionV9> {
        &self.selection
    }
    pub(crate) fn phase(&self) -> WorkflowEditPhaseV9 {
        self.phase
    }
    pub(crate) fn repositories(&self) -> &Arc<WorkflowRemoteRepositoriesV9> {
        &self.repositories
    }
    pub(crate) fn refs(&self) -> &Arc<WorkflowRemoteRefsV9> {
        &self.refs
    }
    pub(crate) fn git(&self) -> &Arc<WorkflowRemoteGitObservationV9> {
        &self.git
    }
    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.retained
    }
    pub(crate) fn published_branch_v9(&self) -> Option<&Arc<WorkflowPublishedBranchV9>> {
        self.branch.as_ref()
    }
    pub(super) fn reserve_attempt_v9(&self) -> bool {
        !self.attempted.swap(true, Ordering::AcqRel)
    }
    pub(super) fn attempted_v9(&self) -> bool {
        self.attempted.load(Ordering::Acquire)
    }
    #[cfg(test)]
    pub(crate) fn prepare(
        selection: Arc<WorkflowRemoteSelectionV9>,
        phase: WorkflowEditPhaseV9,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        Self::prepare_inner(
            selection,
            phase,
            provider,
            database,
            cancel,
            deadline,
            RemotePreviewOwnerV9 {
                budget: WorkflowUiBudgetV9::standalone(),
                branch: None,
                request: None,
            },
        )
    }
    pub(crate) fn prepare_for_form_v9(
        request: &crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditRequestV9,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        use crate::scoped_actions_workflow_edit_ui_v9::{
            WorkflowEditPhaseV9 as UiPhase, WorkflowEditStepV9,
        };
        let phase = match request.step() {
            WorkflowEditStepV9::PublishBranch => WorkflowEditPhaseV9::PublishBranch,
            WorkflowEditStepV9::CreatePr => WorkflowEditPhaseV9::CreatePr,
            _ => return Err(local_edit_refused_v9()),
        };
        let budget = WorkflowUiBudgetV9::for_form(request, UiPhase::Preview, request.step())?;
        let selection = request
            .remote_selection()
            .ok_or_else(local_edit_refused_v9)?;
        Self::prepare_inner(
            selection.clone(),
            phase,
            provider,
            database,
            cancel,
            deadline,
            RemotePreviewOwnerV9 {
                budget,
                branch: request.published_branch_v9().cloned(),
                request: Some(request),
            },
        )
    }
    fn prepare_inner(
        selection: Arc<WorkflowRemoteSelectionV9>,
        remote_phase: WorkflowEditPhaseV9,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        owner: RemotePreviewOwnerV9<'_>,
    ) -> GithubResult<Arc<Self>> {
        let RemotePreviewOwnerV9 {
            budget,
            branch,
            request,
        } = owner;
        shared::clock(cancel, deadline)?;
        if !matches!(
            remote_phase,
            WorkflowEditPhaseV9::PublishBranch | WorkflowEditPhaseV9::CreatePr
        ) || deadline.saturating_duration_since(Instant::now())
            > std::time::Duration::from_secs(30)
        {
            return Err(local_edit_refused_v9());
        }
        let committed = selection.committed();
        let opened = committed.staged().saved().opened();
        let intent = opened.intent();
        if branch.as_ref().is_some_and(|prior| {
            remote_phase != WorkflowEditPhaseV9::CreatePr || !prior.binds_selection_v9(&selection)
        }) {
            return Err(local_edit_refused_v9());
        }
        let validate_local = || match &branch {
            Some(prior) => opened.validate_after_own_branch_v9(database, cancel, prior),
            None => opened.validate_local(database, cancel),
        };
        let base = committed
            .retained_bytes_v9()
            .and_then(|n| n.checked_add(selection.owned_bytes_v9()))
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>() + 4096))
            .ok_or_else(local_edit_refused_v9)?;
        budget.check_peak(
            base.checked_add(4 * 1024 * 1024)
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        let core_policy = match request {
            Some(request) => {
                WorkflowCoreJsonV9::for_preview_v9(provider, request, cancel, deadline)?
            }
            None => WorkflowCoreJsonV9::for_standalone_preview_v9(
                provider, committed, cancel, deadline,
            )?,
        };
        let phase = provider.clone().with_catalogue_read_deadline(deadline);
        let mut session = phase.begin_workflow_edit_access_v9(
            intent.source().original().source(),
            &shared::access_limits(),
            &core_policy,
            cancel,
        )?;
        shared::scope(intent.source(), session.identity())?;
        // Version/host are the actual opening scalar allocations. A fresh
        // closing user document is already capped64KiB by this SAME Core;
        // reserve that full login envelope before every material allocation.
        let identity_reserve = identity_bytes_v9(session.identity())
            .checked_add(64 * 1024)
            .ok_or_else(local_edit_refused_v9)?;
        let base = base
            .checked_add(identity_reserve)
            .ok_or_else(local_edit_refused_v9)?;
        budget.check_peak(
            base.checked_add(4 * 1024 * 1024)
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        session.verify(intent.source().reference(), cancel)?;
        validate_local()?;
        // Preview has no Write/post slots. It cannot seal a future release;
        // Confirm creates exactly one NEW explicit bounded original attempt.
        let (window, post) = session.reserve_workflow_edit_transport_v9(
            intent.original(),
            WorkflowEditPhaseV9::Bind,
            1,
            cancel,
        )?;
        if post.is_some() {
            return Err(local_edit_refused_v9());
        }
        let effective = effective_phase_deadline_v9(cancel, deadline, window.deadline())?;
        let measured = phase.clone().with_measured_read_window(window)?;
        let json_heap = core_policy.bind_measured_v9(&measured, cancel)?;
        // Fixed bounded typed models/rows/Docs are NEW material. Old Form and
        // prior Preview/C/D ownership is already captured once by this ledger.
        let _models = json_heap.reserve_model_v9(1024 * 1024)?;
        let current = session
            .observed_original_v1(intent.source().reference().id)
            .ok_or_else(local_edit_refused_v9)?;
        if let Some(pr) = intent.source().pr_origin_v2() {
            pr.fresh_workflow_edit_parent_v9(
                &measured,
                session.identity(),
                current,
                &json_heap,
                cancel,
            )?;
        }
        let first = measured.scoped_actions_workflow_snapshot_heap_v9(
            current,
            intent.original(),
            true,
            &json_heap,
            cancel,
        )?;
        if !first.same_pin_v6(intent.original(), true) {
            return Err(local_edit_refused_v9());
        }
        let repositories = Arc::new(measured.workflow_remote_repositories_v9(
            intent,
            selection.member(),
            &json_heap,
            cancel,
        )?);
        let repository = opened
            .binding()
            .current_repository_v9(measured.remote_git_read_window_v1(cancel)?, cancel)
            .map_err(|_| local_edit_refused_v9())?;
        let git = WorkflowRemoteGitObservationV9::observe(
            committed,
            &repository,
            repositories.head(),
            selection.remote_name(),
            remote_phase,
        )
        .map_err(|_| local_edit_refused_v9())?;
        let refs = Arc::new(measured.workflow_remote_refs_v9(
            &repositories,
            committed,
            selection.base_branch(),
            remote_phase,
            &json_heap,
            cancel,
        )?);
        if branch
            .as_ref()
            .is_some_and(|prior| !prior.binds_pr_refs_v9(&refs))
        {
            return Err(local_edit_refused_v9());
        }
        let values = committed.staged().saved().values();
        let before_payload = base
            .checked_add(repositories.owned_bytes_v9())
            .and_then(|n| n.checked_add(git.owned_bytes_v9()))
            .and_then(|n| n.checked_add(refs.owned_bytes_v9()))
            .ok_or_else(local_edit_refused_v9)?;
        let mut _payload_heap = None;
        let payload = if remote_phase == WorkflowEditPhaseV9::CreatePr {
            // Same serializer's checked raw*8+64KiB encoded envelope plus
            // original typed request strings/headers. Refuse BEFORE cloning
            // title/body or allocating a JSON tree/encoded payload.
            let payload_bound = values
                .pr_title()
                .len()
                .checked_add(values.pr_body().len())
                .and_then(|n| n.checked_mul(10))
                .and_then(|n| n.checked_add(68 * 1024 + std::mem::size_of::<WorkflowPrPayloadV9>()))
                .ok_or_else(local_edit_refused_v9)?;
            budget.check_retained(
                before_payload
                    .checked_add(payload_bound)
                    .ok_or_else(local_edit_refused_v9)?,
            )?;
            budget.check_peak(
                before_payload
                    .checked_add(payload_bound)
                    .and_then(|n| n.checked_add(4 * 1024 * 1024))
                    .ok_or_else(local_edit_refused_v9)?,
            )?;
            _payload_heap = Some(json_heap.reserve_model_v9(payload_bound)?);
            let payload = WorkflowPrPayloadV9::prepare(&refs, values)?;
            if payload.owned_bytes_v9() > payload_bound {
                return Err(local_edit_refused_v9());
            }
            payload.verify_duplicates_v9(&measured, &json_heap, cancel)?;
            Some(payload)
        } else {
            None
        };
        let material = before_payload
            .checked_add(payload.as_ref().map_or(0, |p| p.owned_bytes_v9()))
            .ok_or_else(local_edit_refused_v9)?;
        // Before cloning private title/body into rows, account their actual
        // expanded source-row headers and bounded formatter slack. No escaped
        // JSON/YAML expansion, hidden truncation or foreground reflow.
        let source_rows = if payload.is_some() {
            values.pr_body().lines().count().checked_add(24)
        } else {
            Some(24)
        }
        .ok_or_else(local_edit_refused_v9)?;
        let source_bytes = if payload.is_some() {
            values.pr_title().len().checked_add(values.pr_body().len())
        } else {
            Some(0)
        }
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(local_edit_refused_v9)?;
        let rows_bound = source_rows
            .checked_mul(std::mem::size_of::<String>())
            .and_then(|n| n.checked_add(source_bytes.checked_mul(2)?))
            .ok_or_else(local_edit_refused_v9)?;
        budget.check_retained(
            material
                .checked_add(rows_bound)
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        budget.check_peak(
            material
                .checked_add(rows_bound)
                .and_then(|n| n.checked_add(4 * 1024 * 1024))
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        let _lines_heap = json_heap.reserve_model_v9(rows_bound)?;
        let mut lines = Vec::with_capacity(source_rows);
        lines.extend([
            if remote_phase == WorkflowEditPhaseV9::PublishBranch { "Preview create-only workflow branch push".into() } else { "Preview exact workflow pull request".into() },
            format!("Base repository: {} • ID {} • workflow {}", repositories.base().name(), repositories.base().id(), intent.original().model().id),
            format!("Actual head member: {} • ID {}", repositories.head().name(), repositories.head().id()),
            format!("Configured remote: {}", git.remote_name()),
            format!("Exact transport URL: {}", git.url()),
            format!("New head branch: {}", refs.head_branch()),
            format!("Actual reviewed parent C: {}", intent.definition().commit_sha),
            format!("Actual reviewed local commit D: {}", committed.commit().head()),
            format!("Chosen base branch: {}", refs.base_branch()),
            format!("Actual observed base B: {}", refs.base_sha()),
            "Fresh explicit Confirm revalidates current source, account, exact refs, config, local bytes and protected journal.".into(),
            "One attempt only. No force replacement, automatic retry, branch cleanup, rollback or permission inferred from names.".into(),
        ]);
        if payload.is_some() {
            lines.push(format!("Exact PR title: {}", values.pr_title()));
            lines.push("Exact PR body (private; not persisted in remote-history metadata):".into());
            lines.extend(values.pr_body().split('\n').map(str::to_owned));
            lines.push("A returned PR receipt means canonical current readback, not successful CI or an atomic causal guarantee.".into());
        } else {
            lines.push("Actual Git exact-ref output was EMPTY and actual bounded API GET returned404; neither alone proves physical absence.".into());
            lines.push("The write uses an EMPTY expected-ref lease; any existing branch refuses replacement.".into());
        }
        let lines_bytes = lines
            .capacity()
            .checked_mul(std::mem::size_of::<String>())
            .and_then(|n| {
                lines
                    .iter()
                    .try_fold(n, |n, row| n.checked_add(row.capacity()))
            })
            .ok_or_else(local_edit_refused_v9)?;
        if lines.len() > source_rows || lines_bytes > rows_bound {
            return Err(local_edit_refused_v9());
        }
        let prepared_base = material
            .checked_add(lines_bytes)
            .ok_or_else(local_edit_refused_v9)?;
        let display_peak = budget
            .peak_remaining(prepared_base)?
            .min(json_heap.remaining_v9()?);
        let mut _display_heap = json_heap.reserve_model_v9(display_peak)?;
        let display = PreparedFormTextV1::prepare_with_remaining_v9(
            &lines,
            cancel,
            effective,
            budget.retained_remaining(prepared_base)?,
            display_peak,
        )
        .map_err(|_| local_edit_refused_v9())?;
        _display_heap.retain_actual_v9(display.retained_bytes())?;
        let retained = prepared_base
            .checked_add(display.retained_bytes())
            .ok_or_else(local_edit_refused_v9)?;
        budget.check_retained(retained)?;
        refs.close(&measured, &json_heap, cancel)?;
        repositories.close(&measured, &json_heap, cancel)?;
        let last = measured.scoped_actions_workflow_snapshot_heap_v9(
            current,
            intent.original(),
            true,
            &json_heap,
            cancel,
        )?;
        if !last.same_pin_v6(intent.original(), true) {
            return Err(local_edit_refused_v9());
        }
        if let Some(pr) = intent.source().pr_origin_v2() {
            pr.fresh_workflow_edit_parent_v9(
                &measured,
                session.identity(),
                current,
                &json_heap,
                cancel,
            )?;
        }
        session.verify(intent.source().reference(), cancel)?;
        let closed = session.close_with_guard(cancel, || {
            validate_local()?;
            committed
                .staged()
                .saved()
                .file()
                .validate(cancel, effective)
                .map_err(|_| local_edit_refused_v9())
        })?;
        shared::scope(intent.source(), closed.identity())?;
        if identity_bytes_v9(closed.identity()) > identity_reserve {
            return Err(local_edit_refused_v9());
        }
        let identity = closed.identity().clone();
        let retained = retained
            .checked_sub(identity_reserve)
            .and_then(|n| n.checked_add(identity_bytes_v9(&identity)))
            .ok_or_else(local_edit_refused_v9)?;
        Ok(Arc::new(Self {
            selection,
            phase: remote_phase,
            repositories,
            refs,
            git,
            _retained_payload: payload,
            lines,
            display,
            _retained_identity: identity,
            retained,
            attempted: AtomicBool::new(false),
            branch,
        }))
    }
}
