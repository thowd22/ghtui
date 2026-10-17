//! Closed current numeric repositories and exact branch observation for W9.
//! gh remains the only API/auth transport; these are not publication permits.
use super::workflow_json_v9::{WorkflowJsonRoleV9, WorkflowJsonValueV9};
use super::*;
use crate::providers::runner::{CommandFailure, CommandKind, FailureKind, ScopedReadBudgetV1};
use crate::scoped_actions_workflow_edit_v9::{WorkflowEditIntentV9, WorkflowHeadMemberV9};
use std::{sync::Arc, time::Instant};
#[path = "workflow_remote_history_v9.rs"]
mod historical_v9;
pub(crate) use historical_v9::{ScopedHistoricalWorkflowRemoteReadV9, WorkflowHistoricalGetV9};

fn refused() -> GithubError {
    error(GithubErrorKind::IdentityChanged, "Original workflow remote repository/ref unavailable or changed; private diagnostics omitted, no fallback")
}
fn clock(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(refused())
    } else {
        Ok(())
    }
}
// Internal implementation only: every call below is constructed by one of
// the genuine fixed W9 repository/ref/payload factories in THIS module.
// There is no exported arbitrary-endpoint or raw transport method.
fn read_json_v9(
    provider: &GithubProvider,
    endpoint: &str,
    role: WorkflowJsonRoleV9,
    heap: &WorkflowJsonLedgerV9,
    cancel: &CancellationToken,
) -> GithubResult<WorkflowJsonValueV9> {
    let capture = heap.capture_v9(provider, role, cancel)?;
    let mut spec = provider.spec(vec![
        "api".into(),
        endpoint.into(),
        "--hostname".into(),
        provider.host.clone(),
        "--method".into(),
        "GET".into(),
    ]);
    spec.output_limit = spec.output_limit.min(capture.cap_v9());
    spec.original_read_cancellation = Some(cancel.clone());
    capture.check_spec_v9(&mut spec)?;
    let bytes = provider
        .runner
        .run(spec, cancel)
        .map_err(|_| refused())?
        .stdout;
    let length = bytes.len();
    let value = capture.decode_owned_v9(bytes, 0..length)?;
    if value
        .value()
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| !errors.is_empty())
    {
        return Err(refused());
    }
    Ok(value)
}
fn positive(raw: &Value, key: &str) -> GithubResult<u64> {
    raw.get(key)
        .and_then(Value::as_u64)
        .filter(|n| (1..=i64::MAX as u64).contains(n))
        .ok_or_else(refused)
}
fn text<'a>(raw: &'a Value, key: &str, cap: usize) -> GithubResult<&'a str> {
    raw.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= cap && !s.chars().any(char::is_control))
        .ok_or_else(refused)
}
pub(crate) struct WorkflowRemoteRepositoryV9 {
    original: Arc<ScopedActionsWorkflowV2>,
    member: Arc<WorkflowHeadMemberV9>,
    producer: CommandRunner,
    pool: Arc<ScopedReadBudgetV1>,
    cancel: CancellationToken,
    deadline: Instant,
    host: String,
    id: u64,
    node: String,
    original_name: String,
    name: String,
    network_id: u64,
    network_node: String,
    clone_url: String,
    ssh_url: String,
    push: bool,
}
impl std::fmt::Debug for WorkflowRemoteRepositoryV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowRemoteRepositoryV9(<actual numeric current observation; no permission renewal>)")
    }
}
impl WorkflowRemoteRepositoryV9 {
    fn bind(
        provider: &GithubProvider,
        intent: &Arc<WorkflowEditIntentV9>,
        member: &Arc<WorkflowHeadMemberV9>,
        raw: &Value,
        head: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<Self>> {
        let (deadline, _) = provider.scoped_read_window.ok_or_else(refused)?;
        let pool = provider.scoped_read_budget.as_ref().ok_or_else(refused)?;
        pool.check_clock(cancel).map_err(|_| refused())?;
        if !member.is_original(intent) || !intent.original().original_writer_v6(provider) {
            return Err(refused());
        }
        let (id, node, original_name) = if head {
            (
                member.row().id,
                member.row().node_id.as_deref().ok_or_else(refused)?,
                member.row().full_name.as_str(),
            )
        } else {
            (
                intent.original().repository().id(),
                intent.original().repository().node().ok_or_else(refused)?,
                intent.source().reference().last_name.as_str(),
            )
        };
        let name = text(raw, "full_name", 256)?;
        validate_repo(name)?;
        if positive(raw, "id")? != id
            || text(raw, "node_id", 256)? != node
            || raw.get("archived").and_then(Value::as_bool) != Some(false)
            || raw.pointer("/permissions/pull").and_then(Value::as_bool) != Some(true)
            || head && raw.pointer("/permissions/push").and_then(Value::as_bool) != Some(true)
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
        let clone_url = text(raw, "clone_url", 1024)?;
        let ssh_url = text(raw, "ssh_url", 1024)?;
        // Only the exact API advertised canonical HTTPS/SSH spellings. No
        // embedded credentials, URL rewrite, port/query or local-path fallback.
        if clone_url != format!("https://{}/{name}.git", provider.host)
            || ssh_url != format!("git@{}:{name}.git", provider.host)
        {
            return Err(refused());
        }
        clock(cancel, deadline)?;
        Ok(Arc::new(Self {
            original: intent.original().clone(),
            member: member.clone(),
            producer: provider.runner.clone(),
            pool: pool.clone(),
            cancel: cancel.clone(),
            deadline,
            host: provider.host.clone(),
            id,
            node: node.into(),
            original_name: original_name.into(),
            name: name.into(),
            network_id,
            network_node: network_node.into(),
            clone_url: clone_url.into(),
            ssh_url: ssh_url.into(),
            push: head,
        }))
    }
    fn check(&self, provider: &GithubProvider, cancel: &CancellationToken) -> GithubResult<()> {
        clock(cancel, self.deadline)?;
        self.pool.check_clock(cancel).map_err(|_| refused())?;
        if self.host != provider.host
            || !self.cancel.same_instance(cancel)
            || !self.producer.same_instance(&provider.runner)
            || provider
                .scoped_read_window
                .is_none_or(|(d, _)| d != self.deadline)
            || provider
                .scoped_read_budget
                .as_ref()
                .is_none_or(|p| !Arc::ptr_eq(p, &self.pool))
            || !self.original.original_writer_v6(provider)
        {
            return Err(refused());
        }
        Ok(())
    }
    pub(crate) fn id(&self) -> u64 {
        self.id
    }
    pub(crate) fn node(&self) -> &str {
        &self.node
    }
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
    pub(crate) fn original_name(&self) -> &str {
        &self.original_name
    }
    pub(crate) fn clone_url(&self) -> &str {
        &self.clone_url
    }
    pub(crate) fn ssh_url(&self) -> &str {
        &self.ssh_url
    }
    pub(crate) fn matches_git_window_v9(
        &self,
        intent: &Arc<WorkflowEditIntentV9>,
        window: &crate::providers::remote_git_read_window_v1::RemoteGitReadWindowV1,
        cancel: &CancellationToken,
    ) -> bool {
        self.member.is_original(intent)
            && Arc::ptr_eq(&self.original, intent.original())
            && self.cancel.same_instance(cancel)
            && self.pool.check_clock(cancel).is_ok()
            && window.same_operation_v2(
                &crate::providers::remote_git_read_window_v1::RemoteGitReadWindowV1::new(
                    self.producer.clone(),
                    self.pool.clone(),
                    self.deadline,
                    1,
                    self.cancel.clone(),
                ),
            )
    }
    pub(crate) fn owned_bytes_v9(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + [
                &self.host,
                &self.node,
                &self.original_name,
                &self.name,
                &self.network_node,
                &self.clone_url,
                &self.ssh_url,
            ]
            .iter()
            .map(|s| s.capacity())
            .sum::<usize>()
    }
}
pub(crate) struct WorkflowRemoteRepositoriesV9 {
    intent: Arc<WorkflowEditIntentV9>,
    member: Arc<WorkflowHeadMemberV9>,
    base: Arc<WorkflowRemoteRepositoryV9>,
    head: Arc<WorkflowRemoteRepositoryV9>,
}
impl WorkflowRemoteRepositoriesV9 {
    pub(crate) fn same_observation_v9(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.intent, &other.intent)
            && self.member.same_member_v9(&other.member)
            && [(&self.base, &other.base), (&self.head, &other.head)]
                .into_iter()
                .all(|(a, b)| {
                    (
                        a.id,
                        &a.node,
                        &a.name,
                        a.network_id,
                        &a.network_node,
                        &a.clone_url,
                        &a.ssh_url,
                        a.push,
                    ) == (
                        b.id,
                        &b.node,
                        &b.name,
                        b.network_id,
                        &b.network_node,
                        &b.clone_url,
                        &b.ssh_url,
                        b.push,
                    )
                })
    }
    pub(crate) fn base(&self) -> &Arc<WorkflowRemoteRepositoryV9> {
        &self.base
    }
    pub(crate) fn head(&self) -> &Arc<WorkflowRemoteRepositoryV9> {
        &self.head
    }
    pub(crate) fn owned_bytes_v9(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.base.owned_bytes_v9()
            + if Arc::ptr_eq(&self.base, &self.head) {
                0
            } else {
                self.head.owned_bytes_v9()
            }
    }
    pub(crate) fn close(
        &self,
        provider: &GithubProvider,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.base.check(provider, cancel)?;
        self.head.check(provider, cancel)?;
        let fresh =
            provider.workflow_remote_repositories_v9(&self.intent, &self.member, heap, cancel)?;
        for (old, new) in [(&self.base, &fresh.base), (&self.head, &fresh.head)] {
            if (
                old.id,
                &old.node,
                &old.name,
                old.network_id,
                &old.network_node,
                &old.clone_url,
                &old.ssh_url,
                old.push,
            ) != (
                new.id,
                &new.node,
                &new.name,
                new.network_id,
                &new.network_node,
                &new.clone_url,
                &new.ssh_url,
                new.push,
            ) {
                return Err(refused());
            }
        }
        Ok(())
    }
}
/// Only an actual fresh repository can create this fixed GET purpose. Neither
/// command args nor raw output are exposed outside the provider boundary.
pub(crate) struct WorkflowExactBranchGetV9 {
    repository: Arc<WorkflowRemoteRepositoryV9>,
    spec: CommandSpec,
}

pub(crate) struct WorkflowRemoteRefsV9 {
    repositories: Arc<WorkflowRemoteRepositoriesV9>,
    committed: Arc<crate::scoped_actions_workflow_edit_v9::WorkflowCommittedLocalV9>,
    phase: WorkflowEditPhaseV9,
    branch: String,
    base_branch: String,
    base_sha: String,
    head_sha: Option<String>,
}
impl std::fmt::Debug for WorkflowRemoteRefsV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "WorkflowRemoteRefsV9(<actual original observed ref bracket; no publication permit>)",
        )
    }
}
impl WorkflowRemoteRefsV9 {
    pub(crate) fn same_observation_v9(&self, other: &Self) -> bool {
        self.repositories.same_observation_v9(&other.repositories)
            && Arc::ptr_eq(&self.committed, &other.committed)
            && self.phase == other.phase
            && self.branch == other.branch
            && self.base_branch == other.base_branch
            && self.base_sha == other.base_sha
            && self.head_sha == other.head_sha
    }
    pub(in crate::providers::gh) fn original_v9(&self) -> &Arc<ScopedActionsWorkflowV2> {
        self.repositories.intent.original()
    }
    pub(in crate::providers::gh) fn validate_post_v9(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        deadline: Instant,
        phase: WorkflowEditPhaseV9,
    ) -> GithubResult<()> {
        let head = self.repositories.head();
        clock(cancel, deadline)?;
        if deadline != head.deadline
            || phase != self.phase
            || !head.cancel.same_instance(cancel)
            || !head.producer.same_instance(&provider.runner)
            || head.host != provider.host
            || !self.original_v9().original_writer_v6(provider)
        {
            return Err(refused());
        }
        Ok(())
    }
    pub(crate) fn repositories(&self) -> &Arc<WorkflowRemoteRepositoriesV9> {
        &self.repositories
    }
    pub(crate) fn committed(
        &self,
    ) -> &Arc<crate::scoped_actions_workflow_edit_v9::WorkflowCommittedLocalV9> {
        &self.committed
    }
    pub(crate) fn base_branch(&self) -> &str {
        &self.base_branch
    }
    pub(crate) fn base_sha(&self) -> &str {
        &self.base_sha
    }
    pub(crate) fn head_branch(&self) -> &str {
        &self.branch
    }
    pub(crate) fn head_sha(&self) -> Option<&str> {
        self.head_sha.as_deref()
    }
    pub(crate) fn phase(&self) -> WorkflowEditPhaseV9 {
        self.phase
    }
    pub(in crate::providers::gh) fn read_paid_head_v9(
        &self,
        provider: &GithubProvider,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<(String, String)> {
        if self.phase != WorkflowEditPhaseV9::PublishBranch {
            return Err(refused());
        }
        let raw = read_json_v9(
            provider,
            &format!(
                "repos/{}/git/ref/heads/{}",
                self.repositories.head().name(),
                url_component(&self.branch)
            ),
            WorkflowJsonRoleV9::ExactBranch,
            heap,
            cancel,
        )?;
        raw.reference_v9(self.repositories.head().name(), &self.branch)
    }
    pub(crate) fn owned_bytes_v9(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.branch.capacity()
            + self.base_branch.capacity()
            + self.base_sha.capacity()
            + self.head_sha.as_ref().map_or(0, String::capacity)
    }
    /// A descriptive SAME-journal descriptor, not an invocation permit. The
    /// only transient payload hash is issued here from the actual bound bytes
    /// and is deliberately absent from all persisted context/fingerprints.
    pub(crate) fn publication_descriptor_v9(
        &self,
        git: &Arc<crate::git::workflow_edit_v9::WorkflowRemoteGitObservationV9>,
        payload: Option<&Arc<WorkflowPrPayloadV9>>,
    ) -> GithubResult<crate::actions_workspace::ActionsPublicationDescriptor> {
        use crate::actions_write_journal::{
            ActionsImmutableContext, ActionsOriginalRepositoryContextV3,
            ActionsOriginalWorkflowContextV6, ActionsOriginalWorkflowEditContextV9,
            ActionsSafeOptions, ActionsWriteIntent, ActionsWriteKind, ActionsWriteTarget,
            WorkflowRemotePhaseV9,
        };
        let source = self.repositories.intent.source();
        let original = self.repositories.intent.original();
        let base = self.repositories.base();
        let head = self.repositories.head();
        head.pool.check_clock(&head.cancel).map_err(|_| refused())?;
        if !Arc::ptr_eq(git.committed(), &self.committed)
            || !Arc::ptr_eq(git.remote(), head)
            || git.deadline() != head.deadline
            || git.phase() != self.phase
            || git.branch() != self.branch
            || !self
                .repositories
                .member
                .is_original(&self.repositories.intent)
        {
            return Err(refused());
        }
        let (phase, kind) = match self.phase {
            WorkflowEditPhaseV9::PublishBranch if payload.is_none() => (
                WorkflowRemotePhaseV9::Branch,
                ActionsWriteKind::PublishWorkflowBranch,
            ),
            WorkflowEditPhaseV9::CreatePr
                if payload.is_some_and(|payload| {
                    std::ptr::eq(self, payload.refs.as_ref())
                        && Arc::ptr_eq(&payload.values, self.committed.staged().saved().values())
                }) =>
            {
                (
                    WorkflowRemotePhaseV9::Pr,
                    ActionsWriteKind::CreateWorkflowPr,
                )
            }
            _ => return Err(refused()),
        };
        let context = ActionsImmutableContext {
            workflow_commit: Some(self.repositories.intent.definition().commit_sha.clone()),
            original_repository_v3: Some(ActionsOriginalRepositoryContextV3 {
                repository_id: base.id(),
                repository_node: base.node().into(),
                original_name: source.reference().last_name.clone(),
            }),
            original_workflow_v6: Some(ActionsOriginalWorkflowContextV6 {
                workflow_id: original.model().id,
                node: original.node_v6().ok_or_else(refused)?.into(),
                path: original.model().path.clone(),
            }),
            original_pr_v3: source.pr_origin_v2().map(|pr| pr.durable_pr_context_v3()),
            workflow_edit_v9: Some(ActionsOriginalWorkflowEditContextV9 {
                phase,
                head_repository_id: head.id(),
                head_repository_node: head.node().into(),
                head_original_name: head.original_name().into(),
                head_current_name: head.name().into(),
                head_ref: self.branch.clone(),
                base_ref: self.base_branch.clone(),
                original_commit: self.repositories.intent.definition().commit_sha.clone(),
                committed_head: self.committed.commit().head().into(),
                base_commit: self.base_sha.clone(),
            }),
            ..Default::default()
        };
        let target = ActionsWriteTarget::from_original_context_v3(
            source.original().scope(),
            &ActionsTarget::Workflow {
                repository: base.name().into(),
                id: original.model().id,
            },
            &context,
        )
        .map_err(|_| refused())?;
        // Private canonical transport/ref/D bytes are transient only; the
        // fixed create-only Git invocation separately binds the exact Preview
        // and released current observation. No URL/input digest is persisted.
        let push_bytes;
        let transient = if let Some(payload) = payload {
            payload.payload.as_ref()
        } else {
            push_bytes = serde_json::to_vec(&(
                git.url(),
                self.head_branch(),
                self.committed.commit().head(),
            ))
            .map_err(|_| refused())?;
            if push_bytes.capacity() > 16 * 1024 {
                return Err(refused());
            }
            push_bytes.as_slice()
        };
        let intent =
            ActionsWriteIntent::new(kind, context, ActionsSafeOptions::default(), transient)
                .map_err(|_| refused())?;
        intent.fingerprint(&target).map_err(|_| refused())?;
        Ok(crate::actions_workspace::ActionsPublicationDescriptor { target, intent })
    }
    pub(crate) fn close(
        &self,
        provider: &GithubProvider,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let fresh = provider.workflow_remote_refs_v9(
            &self.repositories,
            &self.committed,
            &self.base_branch,
            self.phase,
            heap,
            cancel,
        )?;
        if fresh.branch != self.branch
            || fresh.base_branch != self.base_branch
            || fresh.base_sha != self.base_sha
            || fresh.head_sha != self.head_sha
        {
            return Err(refused());
        }
        Ok(())
    }
}

pub(crate) struct WorkflowPrPayloadV9 {
    refs: Arc<WorkflowRemoteRefsV9>,
    values: Arc<crate::scoped_actions_workflow_edit_v9::WorkflowEditValuesV9>,
    request: CreatePr,
    head_name: String,
    payload: Arc<[u8]>,
}
impl std::fmt::Debug for WorkflowPrPayloadV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowPrPayloadV9(<actual original refs; title/body/payload redacted>)")
    }
}
impl WorkflowPrPayloadV9 {
    pub(crate) fn prepare(
        refs: &Arc<WorkflowRemoteRefsV9>,
        values: &Arc<crate::scoped_actions_workflow_edit_v9::WorkflowEditValuesV9>,
    ) -> GithubResult<Arc<Self>> {
        let actual = refs.committed.staged().saved().values();
        // The SAME saved/committed input Arc remains binding through this
        // scoped first workflow. Editing after Commit requires fresh local
        // workflow entry; no saved content or ID can reconstruct this proof.
        if refs.phase != WorkflowEditPhaseV9::CreatePr
            || !Arc::ptr_eq(actual, values)
            || refs.head_sha.as_deref() != Some(refs.committed.commit().head())
        {
            return Err(refused());
        }
        let raw = values
            .pr_title()
            .len()
            .checked_add(values.pr_body().len())
            .ok_or_else(refused)?;
        let bound = raw
            .checked_mul(8)
            .and_then(|n| n.checked_add(64 * 1024))
            .filter(|n| *n <= 1024 * 1024)
            .ok_or_else(refused)?;
        let request = CreatePr {
            repository: refs.repositories.base().name().into(),
            head_repository: refs.repositories.head().name().into(),
            head: refs.branch.clone(),
            base: refs.base_branch.clone(),
            title: values.pr_title().into(),
            body: values.pr_body().into(),
            draft: false,
            expected_head_sha: refs.committed.commit().head().into(),
            expected_base_sha: refs.base_sha.clone(),
        };
        crate::providers::gh::pr::validate_create_pr_request_v9(&request)?;
        let owner = request
            .head_repository
            .split('/')
            .next()
            .ok_or_else(refused)?;
        let head_name = format!("{}:{}", owner.to_ascii_lowercase(), request.head);
        let body = crate::providers::gh::pr::create_pr_payload_v9(&request, &head_name);
        let payload = serde_json::to_vec(&body).map_err(|_| refused())?;
        if payload.capacity() > bound || payload.len() > 1024 * 1024 {
            return Err(refused());
        }
        Ok(Arc::new(Self {
            refs: refs.clone(),
            values: values.clone(),
            request,
            head_name,
            payload: payload.into(),
        }))
    }
    pub(crate) fn refs(&self) -> &Arc<WorkflowRemoteRefsV9> {
        &self.refs
    }
    pub(in crate::providers::gh) fn payload_v9(&self) -> &Arc<[u8]> {
        &self.payload
    }
    pub(in crate::providers::gh) fn endpoint_v9(&self) -> String {
        format!("repos/{}/pulls", self.request.repository)
    }
    pub(crate) fn owned_bytes_v9(&self) -> usize {
        std::mem::size_of::<Self>()
            + 4 * std::mem::size_of::<usize>()
            + self.payload.len()
            + self.head_name.capacity()
            + [
                &self.request.repository,
                &self.request.head_repository,
                &self.request.head,
                &self.request.base,
                &self.request.title,
                &self.request.body,
                &self.request.expected_head_sha,
                &self.request.expected_base_sha,
            ]
            .iter()
            .map(|s| s.capacity())
            .sum::<usize>()
    }
    pub(crate) fn verify_duplicates_v9(
        &self,
        provider: &GithubProvider,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.refs.repositories.base().check(provider, cancel)?;
        let (deadline, cap) = provider.scoped_read_window.ok_or_else(refused)?;
        let bounded = provider
            .clone()
            .with_scoped_read_window(deadline, cap.min(128 * 1024))?;
        let mut complete = false;
        for page in 1..=2 {
            let endpoint = format!(
                "repos/{}/pulls?state=open&head={}&base={}&per_page=100&page={page}",
                self.request.repository,
                url_component(&self.head_name),
                url_component(&self.request.base)
            );
            let raw = read_json_v9(
                &bounded,
                &endpoint,
                WorkflowJsonRoleV9::DuplicateList,
                heap,
                cancel,
            )?;
            let rows = raw
                .value()
                .as_array()
                .filter(|r| r.len() <= 100)
                .ok_or_else(refused)?;
            if !rows.is_empty() {
                return Err(refused());
            }
            // An empty first page is complete. A nonempty result must refuse
            // rather than claim a partial page proves no matching PR.
            if rows.len() < 100 {
                complete = true;
                break;
            }
        }
        if !complete {
            return Err(refused());
        }
        self.refs.repositories.base().check(provider, cancel)
    }
    pub(in crate::providers::gh) fn read_created_pr_v9(
        &self,
        provider: &GithubProvider,
        created: &WorkflowJsonValueV9,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowJsonValueV9> {
        let number = positive(created.value(), "number")?;
        if self.refs.phase != WorkflowEditPhaseV9::CreatePr
            || !created.value().is_object()
            || created.value().get("errors").is_some()
        {
            return Err(refused());
        }
        read_json_v9(
            provider,
            &format!("repos/{}/pulls/{number}", self.request.repository),
            WorkflowJsonRoleV9::CanonicalPr,
            heap,
            cancel,
        )
    }
    pub(in crate::providers::gh) fn bind_created_v9(
        &self,
        provider: &GithubProvider,
        created: &Value,
        current: &Value,
    ) -> GithubResult<WorkflowPrCreatedV9> {
        let id = positive(created, "id")?;
        let node = text(created, "node_id", 256)?;
        let number = positive(created, "number")?;
        for raw in [created, current] {
            let base = raw.get("base").ok_or_else(refused)?;
            let head = raw.get("head").ok_or_else(refused)?;
            let base_repo = base.get("repo").ok_or_else(refused)?;
            let head_repo = head.get("repo").ok_or_else(refused)?;
            if positive(raw, "id")? != id
                || text(raw, "node_id", 256)? != node
                || positive(raw, "number")? != number
                || raw.get("title").and_then(Value::as_str) != Some(self.values.pr_title())
                || raw.get("body").and_then(Value::as_str) != Some(self.values.pr_body())
                || raw.get("draft").and_then(Value::as_bool) != Some(self.request.draft)
                || raw.get("state").and_then(Value::as_str) != Some("open")
                || positive(base_repo, "id")? != self.refs.repositories.base().id()
                || text(base_repo, "node_id", 256)? != self.refs.repositories.base().node()
                || text(base_repo, "full_name", 256)? != self.request.repository
                || positive(head_repo, "id")? != self.refs.repositories.head().id()
                || text(head_repo, "node_id", 256)? != self.refs.repositories.head().node()
                || text(head_repo, "full_name", 256)? != self.request.head_repository
                || text(base, "ref", 256)? != self.request.base
                || text(base, "sha", 64)? != self.request.expected_base_sha
                || text(head, "ref", 256)? != self.request.head
                || text(head, "sha", 64)? != self.request.expected_head_sha
            {
                return Err(refused());
            }
        }
        let summary =
            provider.pr_summary_from_raw_v19(&self.request.repository, number, current)?;
        Ok(WorkflowPrCreatedV9 {
            repository: self.refs.repositories.base().clone(),
            head: self.refs.repositories.head().clone(),
            id,
            node: node.into(),
            number,
            _retained_summary: summary,
        })
    }
}
pub(crate) struct WorkflowPrCreatedV9 {
    repository: Arc<WorkflowRemoteRepositoryV9>,
    head: Arc<WorkflowRemoteRepositoryV9>,
    id: u64,
    node: String,
    number: u64,
    _retained_summary: PrSummary,
}
pub(crate) struct WorkflowBranchObservedV9 {
    refs: Arc<WorkflowRemoteRefsV9>,
}
impl WorkflowBranchObservedV9 {
    pub(in crate::providers::gh) fn after_paid_ref_v9(refs: &Arc<WorkflowRemoteRefsV9>) -> Self {
        Self { refs: refs.clone() }
    }
    pub(crate) fn refs(&self) -> &Arc<WorkflowRemoteRefsV9> {
        &self.refs
    }
    pub(crate) fn receipt_v9(&self) -> crate::actions_write_journal::ActionsReceipt {
        use crate::actions_write_journal::*;
        ActionsReceipt {
            kind: ActionsReceiptKind::WorkflowBranchObserved,
            resource_id: Some(self.refs.repositories.head().id()),
            observed_attempt: None,
            observed_run_id: None,
            original_dispatch_run_v7: None,
            workflow_remote_v9: Some(WorkflowRemoteReceiptV9::Branch {
                head_repository_id: self.refs.repositories.head().id(),
                head_repository_node: self.refs.repositories.head().node().into(),
                head_ref: self.refs.branch.clone(),
                committed_head: self.refs.committed.commit().head().into(),
            }),
        }
    }
}
impl std::fmt::Debug for WorkflowPrCreatedV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowPrCreatedV9(<actual canonical201/readback; private body redacted>)")
    }
}
impl WorkflowPrCreatedV9 {
    pub(crate) fn receipt_v9(
        &self,
        refs: &Arc<WorkflowRemoteRefsV9>,
    ) -> GithubResult<crate::actions_write_journal::ActionsReceipt> {
        use crate::actions_write_journal::*;
        if !Arc::ptr_eq(&self.repository, refs.repositories.base())
            || !Arc::ptr_eq(&self.head, refs.repositories.head())
            || refs.phase != WorkflowEditPhaseV9::CreatePr
        {
            return Err(refused());
        }
        Ok(ActionsReceipt {
            kind: ActionsReceiptKind::WorkflowPrObserved,
            resource_id: Some(self.number),
            observed_attempt: None,
            observed_run_id: None,
            original_dispatch_run_v7: None,
            workflow_remote_v9: Some(WorkflowRemoteReceiptV9::Pr {
                base_repository_id: self.repository.id(),
                base_repository_node: self.repository.node().into(),
                pr_id: self.id,
                pr_node: self.node.clone(),
                number: self.number,
                base_ref: refs.base_branch.clone(),
                base_commit: refs.base_sha.clone(),
                head_repository_id: self.head.id(),
                head_repository_node: self.head.node().into(),
                head_ref: refs.branch.clone(),
                committed_head: refs.committed.commit().head().into(),
            }),
        })
    }
    pub(crate) fn number(&self) -> u64 {
        self.number
    }
}
impl std::fmt::Debug for WorkflowExactBranchGetV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "WorkflowExactBranchGetV9(<closed literal branch GET; private protocol redacted>)",
        )
    }
}
impl WorkflowExactBranchGetV9 {
    fn capture(
        provider: &GithubProvider,
        repository: &Arc<WorkflowRemoteRepositoryV9>,
        branch: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        repository.check(provider, cancel)?;
        crate::providers::gh::remote_repository_v1::validate_reference_name_v7(branch)?;
        if branch.len() > 256 {
            return Err(refused());
        }
        let mut spec = provider.spec(vec![
            "api".into(),
            format!(
                "repos/{}/git/ref/heads/{}",
                repository.name,
                url_component(branch)
            ),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "GET".into(),
            "--include".into(),
        ]);
        spec.output_limit = spec.output_limit.min(16 * 1024);
        spec.original_read_cancellation = Some(cancel.clone());
        if spec.output_limit == 0 || spec.timeout.is_zero() {
            return Err(refused());
        }
        Ok(Self {
            repository: repository.clone(),
            spec,
        })
    }
    pub(in crate::providers) fn runner_spec_v9<'a>(
        &'a self,
        runner: &CommandRunner,
        cancel: &CancellationToken,
    ) -> Result<&'a CommandSpec, CommandFailure> {
        let bad = || CommandFailure {
            kind: FailureKind::ReconciliationRequired,
            message: "Original closed Workflow branch GET unavailable; no raw fallback".into(),
            exit_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            outcome_unknown: false,
        };
        self.repository
            .pool
            .check_clock(cancel)
            .map_err(|_| bad())?;
        if !self.repository.producer.same_instance(runner)
            || !self.repository.cancel.same_instance(cancel)
            || cancel.is_cancelled()
            || Instant::now() >= self.repository.deadline
            || self.spec.absolute_deadline_v1 != Some(self.repository.deadline)
            || self
                .spec
                .original_read_cancellation
                .as_ref()
                .is_none_or(|t| !t.same_instance(cancel))
            || self
                .spec
                .scoped_read_budget
                .as_ref()
                .is_none_or(|p| !Arc::ptr_eq(p, &self.repository.pool))
            || !matches!(self.spec.kind, CommandKind::Read)
            || self.spec.coalesce_read
            || !self.spec.stdin.is_empty()
        {
            return Err(bad());
        }
        Ok(&self.spec)
    }
}
impl GithubProvider {
    fn workflow_present_ref_v9(
        &self,
        repository: &Arc<WorkflowRemoteRepositoryV9>,
        branch: &str,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<String> {
        repository.check(self, cancel)?;
        if branch.is_empty() || branch.len() > 256 {
            return Err(refused());
        }
        let (deadline, cap) = self.scoped_read_window.ok_or_else(refused)?;
        let bounded = self
            .clone()
            .with_scoped_read_window(deadline, cap.min(16 * 1024))?;
        let raw = read_json_v9(
            &bounded,
            &format!(
                "repos/{}/git/ref/heads/{}",
                repository.name(),
                url_component(branch)
            ),
            WorkflowJsonRoleV9::ExactBranch,
            heap,
            cancel,
        )?;
        let (sha, kind) = raw.reference_v9(repository.name(), branch)?;
        repository.check(self, cancel)?;
        if kind != "commit" {
            return Err(refused());
        }
        Ok(sha)
    }
    pub(crate) fn workflow_remote_refs_v9(
        &self,
        repositories: &Arc<WorkflowRemoteRepositoriesV9>,
        committed: &Arc<crate::scoped_actions_workflow_edit_v9::WorkflowCommittedLocalV9>,
        base_branch: &str,
        phase: WorkflowEditPhaseV9,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowRemoteRefsV9> {
        let saved = committed.staged().saved();
        if !Arc::ptr_eq(saved.opened().intent(), &repositories.intent)
            || !matches!(
                phase,
                WorkflowEditPhaseV9::PublishBranch | WorkflowEditPhaseV9::CreatePr
            )
        {
            return Err(refused());
        }
        let branch = saved.values().branch();
        let base_sha =
            self.workflow_present_ref_v9(repositories.base(), base_branch, heap, cancel)?;
        let head_sha = if phase == WorkflowEditPhaseV9::PublishBranch {
            let observed =
                self.workflow_exact_branch_v9(repositories.head(), branch, heap, cancel)?;
            if observed.is_some() {
                return Err(refused());
            }
            None
        } else {
            let observed =
                self.workflow_present_ref_v9(repositories.head(), branch, heap, cancel)?;
            if observed != committed.commit().head() {
                return Err(refused());
            }
            Some(observed)
        };
        Ok(WorkflowRemoteRefsV9 {
            repositories: repositories.clone(),
            committed: committed.clone(),
            phase,
            branch: branch.into(),
            base_branch: base_branch.into(),
            base_sha,
            head_sha,
        })
    }
    pub(crate) fn workflow_remote_repositories_v9(
        &self,
        intent: &Arc<WorkflowEditIntentV9>,
        member: &Arc<WorkflowHeadMemberV9>,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowRemoteRepositoriesV9> {
        if !member.is_original(intent) || !intent.original().original_writer_v6(self) {
            return Err(refused());
        }
        let (deadline, cap) = self.scoped_read_window.ok_or_else(refused)?;
        let bounded = self
            .clone()
            .with_scoped_read_window(deadline, cap.min(64 * 1024))?;
        let base_raw = read_json_v9(
            &bounded,
            &format!("repositories/{}", intent.original().repository().id()),
            WorkflowJsonRoleV9::NumericRepository,
            heap,
            cancel,
        )?;
        if positive(base_raw.value(), "id")? != intent.original().repository().id()
            || Some(text(base_raw.value(), "node_id", 256)?)
                != intent.original().repository().node()
        {
            return Err(refused());
        }
        let same = member.row().id == intent.original().repository().id();
        let base =
            WorkflowRemoteRepositoryV9::bind(self, intent, member, base_raw.value(), same, cancel)?;
        drop(base_raw);
        let head = if same {
            base.clone()
        } else {
            let raw = read_json_v9(
                &bounded,
                &format!("repositories/{}", member.row().id),
                WorkflowJsonRoleV9::NumericRepository,
                heap,
                cancel,
            )?;
            WorkflowRemoteRepositoryV9::bind(self, intent, member, raw.value(), true, cancel)?
        };
        if (base.network_id, &base.network_node) != (head.network_id, &head.network_node) {
            return Err(refused());
        }
        Ok(WorkflowRemoteRepositoriesV9 {
            intent: intent.clone(),
            member: member.clone(),
            base,
            head,
        })
    }
    pub(crate) fn workflow_exact_branch_v9(
        &self,
        repository: &Arc<WorkflowRemoteRepositoryV9>,
        branch: &str,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<Option<String>> {
        let capture = heap.capture_v9(self, WorkflowJsonRoleV9::ExactBranch, cancel)?;
        let mut request = WorkflowExactBranchGetV9::capture(self, repository, branch, cancel)?;
        capture.check_spec_v9(&mut request.spec)?;
        let output = self
            .runner
            .run_workflow_branch_get_v9(&request, cancel)
            .map_err(|_| refused())?;
        repository.check(self, cancel)?;
        let (raw, exit) = output.into_raw_v9();
        let Some(value) = crate::providers::gh::repository_catalogue::workflow_branch_response_v9(
            raw, exit, capture,
        )?
        else {
            return Ok(None);
        };
        // SAME exact-ref grammar and object parser used by native browsing.
        let (sha, kind) = value.reference_v9(repository.name(), branch)?;
        if kind != "commit" {
            return Err(refused());
        }
        Ok(Some(sha))
    }
}
