//! Exact W9 Branch / KNOWN durable PR historical READs. Saved scalars only
//! select reads after fresh numeric observations; never a live write target.
use super::*;
use crate::actions_write_journal::{
    ActionsHistoricalWorkflowRemoteKeyV9, ActionsOriginalWorkflowEditContextV9,
    ActionsPublicationReference, WorkflowRemotePhaseV9, WorkflowRemoteReceiptV9,
};
use crate::providers::runner::CommandKind;

#[derive(PartialEq, Eq)]
struct RepositoryV9 {
    id: u64,
    node: String,
    name: String,
    network_id: u64,
    network_node: String,
}
#[derive(PartialEq, Eq)]
enum ResourceV9 {
    Unavailable,
    Branch,
    Pr {
        id: u64,
        node: String,
        number: u64,
        state: String,
        draft: bool,
        unknown_response: bool,
    },
    UnknownPrEmpty,
}
pub(crate) struct ScopedHistoricalWorkflowRemoteReadV9 {
    producer: CommandRunner,
    host: String,
    reference: ActionsPublicationReference,
    workflow_id: u64,
    edit: ActionsOriginalWorkflowEditContextV9,
    base: RepositoryV9,
    head: RepositoryV9,
    resource: ResourceV9,
    cancellation: CancellationToken,
    deadline: Instant,
    pool: Arc<ScopedReadBudgetV1>,
    pr_barrier: Option<(String, u64)>,
    _model: WorkflowJsonChargeV9,
}
impl ScopedHistoricalWorkflowRemoteReadV9 {
    pub(crate) fn runner_barrier_current_v9(&self) -> bool {
        self.pr_barrier
            .as_ref()
            .is_none_or(|(key, generation)| self.producer.resource_generation(key) == *generation)
    }
    /// This exact typed READ only explains a runner observation. It is called
    /// exclusively by explicit ACK after the full prepared raw-row validation;
    /// it neither authorizes a request nor clears any local-Git protection.
    pub(crate) fn acknowledge_observed_pr_barrier_v9(
        &self,
        provider: &GithubProvider,
        key: &ActionsHistoricalWorkflowRemoteKeyV9,
        cancel: &CancellationToken,
    ) -> anyhow::Result<bool> {
        if !self.matches(provider, key, cancel) {
            anyhow::bail!("Original PR observation generation changed; protection retained");
        }
        let Some((resource, generation)) = &self.pr_barrier else {
            return Ok(false);
        };
        if key.edit().phase != WorkflowRemotePhaseV9::Pr
            || !self
                .producer
                .mark_reconciled_generation(resource, *generation)
        {
            anyhow::bail!("Newer runner generation cannot be acknowledged by old W9 observation");
        }
        Ok(true)
    }
    pub(crate) fn available(&self) -> bool {
        !matches!(self.resource, ResourceV9::Unavailable)
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        32 * 1024
    }
    pub(crate) fn availability_label(&self) -> &'static str {
        if matches!(self.resource, ResourceV9::UnknownPrEmpty) {
            "No matching PR currently observed on two complete exact pages • effect remains UNVERIFIED • no retry/resend"
        } else if self.available() {
            "Actual exact historical remote facts readable • effect/causality UNVERIFIED • READ/ACK only"
        } else {
            "Exact historical endpoint unavailable (HTTP404) • NOT absence/deletion/causal proof • no ACK"
        }
    }
    pub(crate) fn same_observation(&self, other: &Self) -> bool {
        self.producer.same_instance(&other.producer)
            && self.host == other.host
            && self.reference == other.reference
            && self.workflow_id == other.workflow_id
            && self.edit == other.edit
            && self.base == other.base
            && self.head == other.head
            && self.resource == other.resource
            && self.pr_barrier == other.pr_barrier
    }
    pub(crate) fn matches(
        &self,
        provider: &GithubProvider,
        key: &ActionsHistoricalWorkflowRemoteKeyV9,
        cancel: &CancellationToken,
    ) -> bool {
        self.producer.same_instance(&provider.runner)
            && self.runner_barrier_current_v9()
            && self.host == provider.host
            && provider
                .scoped_read_window
                .is_some_and(|(deadline, _)| deadline == self.deadline)
            && provider
                .scoped_read_budget
                .as_ref()
                .is_some_and(|pool| Arc::ptr_eq(pool, &self.pool))
            && self.reference == *key.reference()
            && self.workflow_id == key.workflow_id()
            && &self.edit == key.edit()
            && self.base.id == key.repository_id()
            && self.base.node == key.repository_node()
            && self.head.id == key.edit().head_repository_id
            && self.head.node == key.edit().head_repository_node
            && self.cancellation.same_instance(cancel)
            && !cancel.is_cancelled()
            && Instant::now() < self.deadline
    }
    pub(crate) fn factual_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!(
                "HISTORICAL {:?} / base repository {} / head repository {}",
                self.edit.phase, self.base.id, self.head.id
            ),
            format!("Saved C {}", self.edit.original_commit),
            format!("Saved D {}", self.edit.committed_head),
            format!(
                "Saved base {} / B {}",
                self.edit.base_ref, self.edit.base_commit
            ),
            format!("Saved exact head ref {}", self.edit.head_ref),
        ];
        if let ResourceV9::Pr {
            number,
            state,
            unknown_response,
            ..
        } = &self.resource
        {
            lines.push(format!(
                "Actual {} PR #{number} / state {state} • noncausal observation, not a new source",
                if *unknown_response {
                    "matching"
                } else {
                    "known durable"
                }
            ));
        }
        lines
    }
}

/// Closed normally-reaped GET carrier. Its only constructor below binds actual
/// current numeric observations and a validated current-scope historical key.
pub(crate) struct WorkflowHistoricalGetV9 {
    spec: CommandSpec,
    producer: CommandRunner,
    pool: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl WorkflowHistoricalGetV9 {
    fn capture(
        provider: &GithubProvider,
        key: &ActionsHistoricalWorkflowRemoteKeyV9,
        base: &RepositoryV9,
        head: &RepositoryV9,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        let (deadline, _) = provider.scoped_read_window.ok_or_else(refused)?;
        let pool = provider.scoped_read_budget.as_ref().ok_or_else(refused)?;
        pool.check_clock(cancel).map_err(|_| refused())?;
        if base.id != key.repository_id()
            || base.node != key.repository_node()
            || head.id != key.edit().head_repository_id
            || head.node != key.edit().head_repository_node
            || (base.network_id, &base.network_node) != (head.network_id, &head.network_node)
        {
            return Err(refused());
        }
        let (endpoint, cap) = match key.edit().phase {
            WorkflowRemotePhaseV9::Branch => (
                format!(
                    "repos/{}/git/ref/heads/{}",
                    head.name,
                    url_component(&key.edit().head_ref)
                ),
                16 * 1024,
            ),
            WorkflowRemotePhaseV9::Pr => match key.receipt() {
                Some(WorkflowRemoteReceiptV9::Pr { number, .. }) => {
                    (format!("repos/{}/pulls/{number}", base.name), 128 * 1024)
                }
                None => {
                    let (owner, _) = head.name.split_once('/').ok_or_else(refused)?;
                    (
                        format!(
                            "repos/{}/pulls?state=all&head={}&base={}&per_page=100&page=1",
                            base.name,
                            url_component(&format!("{owner}:{}", key.edit().head_ref)),
                            url_component(&key.edit().base_ref)
                        ),
                        128 * 1024,
                    )
                }
                _ => return Err(refused()),
            },
        };
        let mut spec = provider.spec(vec![
            "api".into(),
            endpoint,
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "GET".into(),
            "--include".into(),
        ]);
        spec.output_limit = spec.output_limit.min(cap);
        spec.original_read_cancellation = Some(cancel.clone());
        clock(cancel, deadline)?;
        Ok(Self {
            spec,
            producer: provider.runner.clone(),
            pool: pool.clone(),
            cancellation: cancel.clone(),
            deadline,
        })
    }
    pub(in crate::providers) fn runner_spec_v9<'a>(
        &'a self,
        runner: &CommandRunner,
        cancel: &CancellationToken,
    ) -> Result<&'a CommandSpec, CommandFailure> {
        let bad = || CommandFailure {
            kind: FailureKind::ReconciliationRequired,
            message: "Original closed historical read unavailable; no raw fallback".into(),
            exit_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            outcome_unknown: false,
        };
        self.pool.check_clock(cancel).map_err(|_| bad())?;
        if !self.producer.same_instance(runner)
            || !self.cancellation.same_instance(cancel)
            || cancel.is_cancelled()
            || Instant::now() >= self.deadline
            || self.spec.absolute_deadline_v1 != Some(self.deadline)
            || self
                .spec
                .original_read_cancellation
                .as_ref()
                .is_none_or(|t| !t.same_instance(cancel))
            || self
                .spec
                .scoped_read_budget
                .as_ref()
                .is_none_or(|p| !Arc::ptr_eq(p, &self.pool))
            || !matches!(self.spec.kind, CommandKind::Read)
            || self.spec.coalesce_read
            || !self.spec.stdin.is_empty()
            || !self.spec.sensitive_values.is_empty()
        {
            return Err(bad());
        }
        Ok(&self.spec)
    }
}
fn repository(
    provider: &GithubProvider,
    id: u64,
    node: &str,
    heap: &WorkflowJsonLedgerV9,
    cancel: &CancellationToken,
) -> GithubResult<RepositoryV9> {
    let raw = read_json_v9(
        provider,
        &format!("repositories/{id}"),
        WorkflowJsonRoleV9::NumericRepository,
        heap,
        cancel,
    )?;
    let raw = raw.value();
    let name = text(raw, "full_name", 256)?;
    validate_repo(name)?;
    if positive(raw, "id")? != id
        || text(raw, "node_id", 256)? != node
        || raw.pointer("/permissions/pull").and_then(Value::as_bool) != Some(true)
    {
        return Err(refused());
    }
    let (network_id, network_node) = match raw.get("fork").and_then(Value::as_bool) {
        Some(false) => (id, node),
        Some(true) => {
            let root = raw.get("source").ok_or_else(refused)?;
            (positive(root, "id")?, text(root, "node_id", 256)?)
        }
        None => return Err(refused()),
    };
    Ok(RepositoryV9 {
        id,
        node: node.into(),
        name: name.into(),
        network_id,
        network_node: network_node.into(),
    })
}
fn resource(
    provider: &GithubProvider,
    key: &ActionsHistoricalWorkflowRemoteKeyV9,
    base: &RepositoryV9,
    head: &RepositoryV9,
    raw: Option<WorkflowJsonValueV9>,
    heap: &WorkflowJsonLedgerV9,
) -> GithubResult<ResourceV9> {
    let Some(raw) = raw else {
        return Ok(ResourceV9::Unavailable);
    };
    if key.edit().phase == WorkflowRemotePhaseV9::Branch {
        let (sha, kind) = raw.reference_v9(&head.name, &key.edit().head_ref)?;
        if kind != "commit" || sha != key.edit().committed_head {
            return Err(refused());
        }
        return Ok(ResourceV9::Branch);
    }
    let unknown = key.receipt().is_none();
    let raw = if unknown {
        let rows = raw
            .value()
            .as_array()
            .filter(|rows| rows.len() < 100)
            .ok_or_else(refused)?;
        match rows.as_slice() {
            [] => return Ok(ResourceV9::UnknownPrEmpty),
            [one] => one,
            _ => return Err(refused()),
        }
    } else {
        raw.value()
    };
    let (pr_id, pr_node, number) = match key.receipt() {
        Some(WorkflowRemoteReceiptV9::Pr {
            pr_id,
            pr_node,
            number,
            ..
        }) => (*pr_id, pr_node.as_str(), *number),
        None => (
            positive(raw, "id")?,
            text(raw, "node_id", 256)?,
            positive(raw, "number")?,
        ),
        _ => return Err(refused()),
    };
    let rb = raw.get("base").ok_or_else(refused)?;
    let rh = raw.get("head").ok_or_else(refused)?;
    let br = rb.get("repo").ok_or_else(refused)?;
    let hr = rh.get("repo").ok_or_else(refused)?;
    if positive(raw, "id")? != pr_id
        || text(raw, "node_id", 256)? != pr_node
        || positive(raw, "number")? != number
        || positive(br, "id")? != base.id
        || text(br, "node_id", 256)? != base.node
        || text(br, "full_name", 256)? != base.name
        || positive(hr, "id")? != head.id
        || text(hr, "node_id", 256)? != head.node
        || text(hr, "full_name", 256)? != head.name
        || text(rb, "ref", 256)? != key.edit().base_ref
        || text(rb, "sha", 64)? != key.edit().base_commit
        || text(rh, "ref", 256)? != key.edit().head_ref
        || text(rh, "sha", 64)? != key.edit().committed_head
    {
        return Err(refused());
    }
    // SAME canonical PR parser; title/body do NOT become saved approval or get
    // replayed. Unknown states refuse rather than acquire a fallback meaning.
    // Actual canonical typed model copies may coexist with raw+Value. Charge
    // that bounded temporary graph BEFORE the SAME existing parser allocates.
    let _canonical_model = heap.reserve_model_v9(2 * 128 * 1024 + 32 * 1024)?;
    let summary = provider.pr_summary_from_raw_v19(&base.name, number, raw)?;
    let state = text(raw, "state", 16)?;
    if !matches!(state, "open" | "closed")
        || summary.head_sha != key.edit().committed_head
        || summary.base_sha != key.edit().base_commit
    {
        return Err(refused());
    }
    Ok(ResourceV9::Pr {
        id: pr_id,
        node: pr_node.into(),
        number,
        state: state.into(),
        draft: raw
            .get("draft")
            .and_then(Value::as_bool)
            .ok_or_else(refused)?,
        unknown_response: unknown,
    })
}
fn get_resource(
    provider: &GithubProvider,
    key: &ActionsHistoricalWorkflowRemoteKeyV9,
    base: &RepositoryV9,
    head: &RepositoryV9,
    heap: &WorkflowJsonLedgerV9,
    cancel: &CancellationToken,
) -> GithubResult<ResourceV9> {
    let role = match (key.edit().phase, key.receipt()) {
        (WorkflowRemotePhaseV9::Branch, _) => WorkflowJsonRoleV9::HistoricalBranch,
        (WorkflowRemotePhaseV9::Pr, None) => WorkflowJsonRoleV9::HistoricalUnknownPr,
        (WorkflowRemotePhaseV9::Pr, Some(_)) => WorkflowJsonRoleV9::HistoricalPr,
    };
    let capture = heap.capture_v9(provider, role, cancel)?;
    let mut request = WorkflowHistoricalGetV9::capture(provider, key, base, head, cancel)?;
    capture.check_spec_v9(&mut request.spec)?;
    let (bytes, exit) = provider
        .runner
        .run_workflow_historical_get_v9(&request, cancel)
        .map_err(|_| refused())?
        .into_raw_v9();
    drop(request);
    let raw = crate::providers::gh::repository_catalogue::workflow_history_response_v9(
        bytes, exit, capture,
    )?;
    resource(provider, key, base, head, raw, heap)
}
impl GithubProvider {
    pub(crate) fn scoped_actions_workflow_remote_history_v9(
        &self,
        current: &CurrentCatalogueRepository,
        key: &ActionsHistoricalWorkflowRemoteKeyV9,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedHistoricalWorkflowRemoteReadV9> {
        let (deadline, _) = self.scoped_read_window.ok_or_else(refused)?;
        let pool = self.scoped_read_budget.as_ref().ok_or_else(refused)?;
        pool.check_clock(cancel).map_err(|_| refused())?;
        if current.repository().id != key.repository_id()
            || current.repository().node_id.as_deref() != Some(key.repository_node())
            || current.repository().permissions.pull != Some(true)
        {
            return Err(refused());
        }
        let model = heap.reserve_model_v9(32 * 1024)?;
        let base = repository(
            self,
            key.repository_id(),
            key.repository_node(),
            heap,
            cancel,
        )?;
        let head = repository(
            self,
            key.edit().head_repository_id,
            &key.edit().head_repository_node,
            heap,
            cancel,
        )?;
        let pr_barrier = if key.edit().phase == WorkflowRemotePhaseV9::Pr {
            if key.workflow_id() == 0 {
                return Err(refused());
            }
            let resource = format!(
                "{}/repository-id/{}/actions/workflow/{}",
                self.host,
                base.id,
                key.workflow_id()
            );
            let generation = self.runner.resource_generation(&resource);
            Some((resource, generation))
        } else {
            None
        };
        let resource = get_resource(self, key, &base, &head, heap, cancel)?;
        if pr_barrier.as_ref().is_some_and(|(resource, generation)| {
            self.runner.resource_generation(resource) != *generation
        }) {
            return Err(refused());
        }
        clock(cancel, deadline)?;
        Ok(ScopedHistoricalWorkflowRemoteReadV9 {
            producer: self.runner.clone(),
            host: self.host.clone(),
            reference: key.reference().clone(),
            workflow_id: key.workflow_id(),
            edit: key.edit().clone(),
            base,
            head,
            resource,
            cancellation: cancel.clone(),
            deadline,
            pool: pool.clone(),
            pr_barrier,
            _model: model,
        })
    }
    pub(crate) fn close_workflow_remote_history_v9(
        &self,
        original: &ScopedHistoricalWorkflowRemoteReadV9,
        current: &CurrentCatalogueRepository,
        key: &ActionsHistoricalWorkflowRemoteKeyV9,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if !original.matches(self, key, cancel)
            || current.repository().id != key.repository_id()
            || current.repository().node_id.as_deref() != Some(key.repository_node())
            || current.repository().permissions.pull != Some(true)
        {
            return Err(refused());
        }
        // Exact approved role order: resource, base repository, head repository.
        // Retained original numeric/name observations select only this read;
        // both names/network/IDs close again before accepting the observation.
        let resource = get_resource(self, key, &original.base, &original.head, heap, cancel)?;
        let base = repository(
            self,
            key.repository_id(),
            key.repository_node(),
            heap,
            cancel,
        )?;
        let head = repository(
            self,
            key.edit().head_repository_id,
            &key.edit().head_repository_node,
            heap,
            cancel,
        )?;
        if base != original.base
            || head != original.head
            || resource != original.resource
            || !original.runner_barrier_current_v9()
        {
            return Err(refused());
        }
        clock(cancel, original.deadline)
    }
}
