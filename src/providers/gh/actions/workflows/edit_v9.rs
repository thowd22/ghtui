//! One original workflow/immutable-definition READ bracket for local editing.
//! Not a source constructor, write permit, renewed clock or raw transport API.
use super::*;
use std::{sync::Arc, time::Instant};

pub(crate) struct WorkflowDefinitionReadV9 {
    original: Arc<ScopedActionsWorkflowV2>,
    producer: CommandRunner,
    repository: String,
    cancel: CancellationToken,
    deadline: Instant,
}
impl std::fmt::Debug for WorkflowDefinitionReadV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowDefinitionReadV9(<actual read bracket; no permission>)")
    }
}
fn refused() -> GithubError {
    error(GithubErrorKind::IdentityChanged, "Original workflow/ref/definition/principal unavailable; private YAML omitted and no fallback")
}
fn clock(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(refused())
    } else {
        Ok(())
    }
}
// Module-private fixed call sites below derive these endpoints only from the
// genuine Workflow/Definition bracket. Neither raw values nor transport escape.
fn definition_json_v9(
    provider: &GithubProvider,
    endpoint: String,
    role: WorkflowJsonRoleV9,
    heap: &WorkflowJsonLedgerV9,
    cancel: &CancellationToken,
) -> GithubResult<WorkflowJsonValueV9> {
    let capture = heap.capture_v9(provider, role, cancel)?;
    let mut spec = provider.spec(vec![
        "api".into(),
        endpoint,
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
    capture.decode_owned_v9(bytes, 0..length)
}
struct DefinitionInputsV9<'a> {
    current: &'a CurrentCatalogueRepository,
    original: &'a Arc<ScopedActionsWorkflowV2>,
    definition: &'a Arc<WorkflowDefinition>,
    identity: &'a HostIdentity,
}
enum DefinitionBytesV9 {
    Legacy(Vec<u8>),
    Guarded(WorkflowJsonRawV9),
}
impl DefinitionBytesV9 {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Legacy(raw) => raw,
            Self::Guarded(raw) => raw,
        }
    }
}
impl WorkflowDefinitionReadV9 {
    /// Consumes this actual opening witness, so the fixed closing GET cannot
    /// be reused as a general endpoint or reset after a partial failure.
    #[cfg(test)]
    pub(crate) fn close(
        self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.close_inner_v9(provider, None, cancel)
    }
    pub(crate) fn close_with_heap_v9(
        self,
        provider: &GithubProvider,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.close_inner_v9(provider, Some(heap), cancel)
    }
    fn close_inner_v9(
        self,
        provider: &GithubProvider,
        heap: Option<&WorkflowJsonLedgerV9>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        clock(cancel, self.deadline)?;
        if !self.cancel.same_instance(cancel)
            || !self.producer.same_instance(&provider.runner)
            || !self.original.original_writer_v6(provider)
            || provider
                .scoped_read_window
                .is_none_or(|(d, _)| d != self.deadline)
        {
            return Err(refused());
        }
        if let Some(heap) = heap {
            let raw = definition_json_v9(
                provider,
                format!(
                    "repos/{}/actions/workflows/{}",
                    self.repository,
                    self.original.model().id
                ),
                WorkflowJsonRoleV9::WorkflowSnapshot,
                heap,
                cancel,
            )?;
            self.original.bind_raw_v6(provider, raw.value(), true)?;
        } else {
            let raw = provider.scoped_workflow_raw_v6(
                &self.repository,
                self.original.model().id,
                cancel,
            )?;
            self.original.bind_raw_v6(provider, &raw, true)?;
        }
        clock(cancel, self.deadline)
    }
}
impl GithubProvider {
    /// Restriction-only queued bound for W9 draft ordering; it is not a Core,
    /// window, source or account capability. Later actual measured/cursor I
    /// equality remains mandatory for every operation and filesystem write.
    pub(crate) fn workflow_edit_queued_deadline_v9(
        &self,
        requested: Instant,
        cancel: &CancellationToken,
    ) -> GithubResult<Instant> {
        clock(cancel, requested)?;
        if requested.saturating_duration_since(Instant::now()) > std::time::Duration::from_secs(30)
            || self.scoped_read_budget.is_some()
        {
            return Err(refused());
        }
        let actual = self
            .scoped_read_window
            .map_or(requested, |(deadline, _)| deadline.min(requested));
        clock(cancel, actual)?;
        Ok(actual)
    }
    #[cfg(test)]
    pub(crate) fn open_workflow_definition_edit_v9(
        &self,
        current: &CurrentCatalogueRepository,
        original: &Arc<ScopedActionsWorkflowV2>,
        definition: &Arc<WorkflowDefinition>,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowDefinitionReadV9> {
        self.open_workflow_definition_edit_inner_v9(
            DefinitionInputsV9 {
                current,
                original,
                definition,
                identity,
            },
            None,
            cancel,
        )
    }
    pub(crate) fn open_workflow_definition_edit_with_heap_v9(
        &self,
        current: &CurrentCatalogueRepository,
        original: &Arc<ScopedActionsWorkflowV2>,
        definition: &Arc<WorkflowDefinition>,
        identity: &HostIdentity,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowDefinitionReadV9> {
        self.open_workflow_definition_edit_inner_v9(
            DefinitionInputsV9 {
                current,
                original,
                definition,
                identity,
            },
            Some(heap),
            cancel,
        )
    }
    fn open_workflow_definition_edit_inner_v9(
        &self,
        inputs: DefinitionInputsV9<'_>,
        heap: Option<&WorkflowJsonLedgerV9>,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowDefinitionReadV9> {
        let DefinitionInputsV9 {
            current,
            original,
            definition,
            identity,
        } = inputs;
        let (deadline, cap) = self.scoped_read_window.ok_or_else(refused)?;
        clock(cancel, deadline)?;
        let cli = GhCliVersion::parse_line(&identity.gh_version).map_err(|_| refused())?;
        if !matches!(
            CliRequirement::RawEscapeOptOut.assess(Some(&cli)),
            LocalCliSyntax::KnownDocumentedSyntax { .. }
        ) || identity.host != self.host
            || identity.account_id.is_none_or(|uid| uid == 0)
            || !original.matches_dispatch_pool_v7(self, current)
            || definition.workflow.id != original.model().id
            || definition.workflow.path != original.model().path
            || definition.workflow.state != original.model().state
            || !crate::git::original_actor_saved_sha(&definition.commit_sha)
            || definition.yaml.is_empty()
            || definition.yaml.len() > 256 * 1024
        {
            return Err(refused());
        }
        let repository = &current.repository().full_name;
        let opening = if let Some(heap) = heap {
            let raw = definition_json_v9(
                self,
                format!(
                    "repos/{repository}/actions/workflows/{}",
                    original.model().id
                ),
                WorkflowJsonRoleV9::WorkflowSnapshot,
                heap,
                cancel,
            )?;
            original.bind_raw_v6(self, raw.value(), true)?
        } else {
            let raw = self.scoped_workflow_raw_v6(repository, original.model().id, cancel)?;
            original.bind_raw_v6(self, &raw, true)?
        };
        let sha = if let Some(heap) = heap {
            validate_repo(repository)?;
            nonempty_parameter(&definition.revision)?;
            let raw = definition_json_v9(
                self,
                format!(
                    "repos/{repository}/commits/{}",
                    url_component(&definition.revision)
                ),
                WorkflowJsonRoleV9::LocalDefinitionRevision,
                heap,
                cancel,
            )?;
            let sha = text(raw.value(), "sha");
            validate_sha(&sha)?;
            sha
        } else {
            self.revision_sha(repository, &definition.revision, cancel)?
        };
        if sha != definition.commit_sha {
            return Err(refused());
        }
        let path = opening
            .model()
            .path
            .split('/')
            .map(url_component)
            .collect::<Vec<_>>()
            .join("/");
        let capture = heap
            .map(|heap| heap.capture_v9(self, WorkflowJsonRoleV9::LocalDefinitionYaml, cancel))
            .transpose()?;
        let bounded = self
            .clone()
            .with_scoped_read_window(deadline, cap.min(256 * 1024))?;
        let mut spec = bounded.spec(vec![
            "api".into(),
            format!("repos/{repository}/contents/{path}?ref={sha}"),
            "--hostname".into(),
            self.host.clone(),
            "--method".into(),
            "GET".into(),
            "--header".into(),
            "Accept: application/vnd.github.raw+json".into(),
            "--allow-escape-sequences".into(),
        ]);
        if let Some(capture) = &capture {
            spec.output_limit = spec.output_limit.min(capture.cap_v9());
            spec.original_read_cancellation = Some(cancel.clone());
            capture.check_spec_v9(&mut spec)?;
        }
        // SAME spec/runner/measured pool; strict one-shot purpose, unlike the
        // unchanged legacy raw_spec flag fallback. No hidden retry child.
        let bytes = bounded
            .runner
            .run(spec, cancel)
            .map_err(|_| refused())?
            .stdout;
        let bytes = match capture {
            Some(capture) => DefinitionBytesV9::Guarded(capture.own_raw_v9(bytes)?),
            None => DefinitionBytesV9::Legacy(bytes),
        };
        if bytes.bytes() != definition.yaml.as_bytes() {
            return Err(refused());
        }
        WorkflowDefinition::validate_edit_yaml_v9(
            std::str::from_utf8(bytes.bytes()).map_err(|_| refused())?,
        )
        .map_err(|_| refused())?;
        clock(cancel, deadline)?;
        Ok(WorkflowDefinitionReadV9 {
            original: opening,
            producer: self.runner.clone(),
            repository: repository.clone(),
            cancel: cancel.clone(),
            deadline,
        })
    }
}
