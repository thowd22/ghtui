//! Opaque original-repository Actions reads. Existing parsers/transport only.
use super::*;
use std::{collections::BTreeSet, sync::Arc};

pub struct ScopedActionsRepositoryV2 {
    producer: CommandRunner,
    id: u64,
    node: Option<String>,
    name: String,
    default_branch: Option<String>,
}
impl ScopedActionsRepositoryV2 {
    pub(crate) fn owned_bytes_v2(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.node.as_ref().map_or(0, String::capacity)
            + self.name.capacity()
            + self.default_branch.as_ref().map_or(0, String::capacity)
    }
    fn from_current(
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
    ) -> GithubResult<Arc<Self>> {
        let row = current.repository();
        if row.id == 0 || row.permissions.pull != Some(true) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Current original repository with pull access required",
            ));
        }
        Ok(Arc::new(Self {
            producer: provider.runner.clone(),
            id: row.id,
            node: row.node_id.clone(),
            name: row.full_name.clone(),
            default_branch: row.default_branch.clone(),
        }))
    }
    fn matches(&self, provider: &GithubProvider, current: &CurrentCatalogueRepository) -> bool {
        self.producer.same_instance(&provider.runner)
            && self.id == current.repository().id
            && self.node == current.repository().node_id
            && current.repository().permissions.pull == Some(true)
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn node(&self) -> Option<&str> {
        self.node.as_deref()
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn default_branch(&self) -> Option<&str> {
        self.default_branch.as_deref()
    }
}
pub struct ScopedActionsArtifactV2 {
    repository: Arc<ScopedActionsRepositoryV2>,
    run: Arc<ScopedActionsRunTargetV1>,
    node: Option<String>,
    model: ActionsArtifact,
}
impl ScopedActionsArtifactV2 {
    pub(crate) fn matches_archive_pool_v2(
        &self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
    ) -> bool {
        self.repository.matches(provider, current)
            && self.run.current_resource_v2(provider, current)
            && self.repository.id == self.run.repository_id()
            && current.original().id == self.repository.id
            && current
                .original()
                .node_id
                .as_deref()
                .is_none_or(|node| Some(node) == self.repository.node())
    }
    pub(crate) fn owned_bytes_v2(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.node.as_ref().map_or(0, String::capacity)
            + artifact_model_bytes_v2(&self.model)
    }
    pub fn model(&self) -> &ActionsArtifact {
        &self.model
    }
    pub fn repository(&self) -> &Arc<ScopedActionsRepositoryV2> {
        &self.repository
    }
    pub fn run(&self) -> &Arc<ScopedActionsRunTargetV1> {
        &self.run
    }
}
pub struct ScopedActionsWorkflowV2 {
    repository: Arc<ScopedActionsRepositoryV2>,
    node: Option<String>,
    model: Workflow,
}
impl ScopedActionsWorkflowV2 {
    pub(crate) fn owned_bytes_v2(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.node.as_ref().map_or(0, String::capacity)
            + self.model.name.capacity()
            + self.model.path.capacity()
            + self.model.state.capacity()
    }
    pub fn model(&self) -> &Workflow {
        &self.model
    }
    pub fn repository(&self) -> &Arc<ScopedActionsRepositoryV2> {
        &self.repository
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedActionsRepositoryReadV2 {
    Secrets,
    Variables,
    Policy(PolicySection),
    Environments,
    Runners,
    Caches,
}
pub enum ScopedActionsResourceBodyV2 {
    Artifacts {
        run: Arc<ScopedActionsRunTargetV1>,
        targets: Arc<[Arc<ScopedActionsArtifactV2>]>,
    },
    Artifact {
        target: Arc<ScopedActionsArtifactV2>,
        archive: Arc<ArtifactArchive>,
    },
    Workflows(Arc<[Arc<ScopedActionsWorkflowV2>]>),
    Definition {
        target: Arc<ScopedActionsWorkflowV2>,
        definition: Arc<WorkflowDefinition>,
    },
    Secrets(Vec<SecretMetadata>),
    Variables(Vec<ActionsVariable>),
    Policy(Box<PolicySnapshot>),
    Environments(Vec<Value>),
    Runners(Vec<SelfHostedRunner>),
    Caches {
        entries: Vec<ActionCache>,
        usage: Option<CacheUsage>,
    },
    Unavailable(&'static str),
}
pub struct ScopedActionsResourceReadV2 {
    repository: Arc<ScopedActionsRepositoryV2>,
    body: ScopedActionsResourceBodyV2,
}
impl ScopedActionsResourceReadV2 {
    pub fn repository(&self) -> &Arc<ScopedActionsRepositoryV2> {
        &self.repository
    }
    pub fn body(&self) -> &ScopedActionsResourceBodyV2 {
        &self.body
    }
}
pub(crate) fn artifact_model_bytes_v2(model: &ActionsArtifact) -> usize {
    model.name.capacity()
        + model.head_sha.capacity()
        + model.created_at.capacity()
        + model.updated_at.capacity()
        + model.expires_at.capacity()
        + model.digest.as_ref().map_or(0, String::capacity)
}
fn node(raw: &Value) -> GithubResult<Option<String>> {
    match raw.get("node_id") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s))
            if !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control) =>
        {
            Ok(Some(s.clone()))
        }
        _ => Err(error(
            GithubErrorKind::Json,
            "Actions resource node is not a bounded observed identity",
        )),
    }
}
fn unavailable(e: &GithubError) -> bool {
    matches!(
        e.kind,
        GithubErrorKind::Permission | GithubErrorKind::NotFound | GithubErrorKind::Unsupported
    )
}
fn end(cancel: &CancellationToken) -> GithubResult<()> {
    if cancel.is_cancelled() {
        Err(error(
            GithubErrorKind::Transport,
            "Original Actions resource read canceled",
        ))
    } else {
        Ok(())
    }
}
fn bound_rows<T>(rows: &[T]) -> GithubResult<()> {
    if rows.len() > 1000 {
        Err(error(
            GithubErrorKind::Unsupported,
            "Actions resource list exceeds1000 actual members; no silent complete view",
        ))
    } else {
        Ok(())
    }
}
fn artifact(
    raw: &Value,
    repo: &ScopedActionsRepositoryV2,
    run: &ScopedActionsRunTargetV1,
) -> GithubResult<(ActionsArtifact, Option<String>)> {
    let value = super::artifacts::artifact_from_json(raw);
    if value.id == 0
        || value.run_id != run.run().id
        || value.head_sha != run.run().head_sha
        || raw["workflow_run"]["repository_id"].as_u64() != Some(repo.id)
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Actual artifact ID/run/head/numeric repository differs; no attempt inferred",
        ));
    }
    if raw["workflow_run"].get("run_attempt").is_some() && value.attempt.is_none_or(|a| a == 0) {
        return Err(error(
            GithubErrorKind::Json,
            "Supplied artifact attempt is invalid; omission is not a default",
        ));
    }
    if value
        .attempt
        .is_some_and(|attempt| attempt != run.run().attempt)
    {
        return Err(error(GithubErrorKind::IdentityChanged,"Supplied artifact attempt differs from original actual run; omission is not a guessed attempt"));
    }
    Ok((value, node(raw)?))
}
impl GithubProvider {
    /// Fresh genuine original PR/check closure around every Actions resource
    /// continuation. Uses the SAME raw parent parser and actual captured
    /// producer; no display/DTO facts can supply this binding.
    pub(crate) fn scoped_actions_pr_parent_v2(
        &self,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        resource: &PrResourceReadV1,
        index: usize,
        observed: &Arc<PrCheckRunObservation>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if !self.runner.same_instance(observed.producing_runner_v2())
            || !observed.matches_resource_v1(identity, resource, index)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original PR Actions source/principal differs",
            ));
        }
        let raw = self.api(
            &format!(
                "repos/{}/pulls/{}",
                current.repository().full_name,
                observed.pr_number()
            ),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        resource.validate_check_parent_v1(self, current, &raw)?;
        if raw["id"].as_u64() != Some(observed.pr_database_id())
            || raw["node_id"].as_str() != Some(observed.pr_node())
            || raw["head"]["sha"].as_str() != Some(observed.head_sha())
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original PR database/node/head changed before Actions resource",
            ));
        }
        let check = resource.checks().get(index).ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Original CheckRun unavailable",
            )
        })?;
        let raw = self.api(
            &format!(
                "repos/{}/check-runs/{}",
                current.repository().full_name,
                check.id
            ),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if raw["id"].as_u64() != Some(check.id)
            || raw["head_sha"].as_str() != Some(observed.head_sha())
            || raw["node_id"].as_str() != Some(observed.check_node())
            || raw["check_suite"]["id"].as_u64() != Some(observed.suite_id())
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original CheckRun node/suite/head changed",
            ));
        }
        end(cancel)
    }
    pub fn scoped_actions_artifacts_v2(
        &self,
        current: &CurrentCatalogueRepository,
        run: Arc<ScopedActionsRunTargetV1>,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedActionsResourceReadV2> {
        let repository = ScopedActionsRepositoryV2::from_current(self, current)?;
        if !run.current_resource_v2(self, current) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual original run proof required before artifact list",
            ));
        }
        self.scoped_resource_run_current_v2(current, &run, cancel)?;
        let raw = self.api_list(
            &format!(
                "repos/{}/actions/runs/{}/artifacts?per_page=100",
                repository.name,
                run.run().id
            ),
            Some("artifacts"),
            cancel,
        )?;
        bound_rows(&raw)?;
        let mut ids = BTreeSet::new();
        let mut targets = Vec::with_capacity(raw.len());
        for row in &raw {
            let (model, node) = artifact(row, &repository, &run)?;
            if !ids.insert(model.id) {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Duplicate artifact identity in actual inventory",
                ));
            }
            targets.push(Arc::new(ScopedActionsArtifactV2 {
                repository: repository.clone(),
                run: run.clone(),
                model,
                node,
            }));
        }
        self.scoped_resource_run_current_v2(current, &run, cancel)?;
        end(cancel)?;
        Ok(ScopedActionsResourceReadV2 {
            repository,
            body: ScopedActionsResourceBodyV2::Artifacts {
                run,
                targets: targets.into(),
            },
        })
    }
    fn scoped_artifact_current_v2(
        &self,
        current: &CurrentCatalogueRepository,
        target: &ScopedActionsArtifactV2,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if !target.repository.matches(self, current)
            || !target.run.current_resource_v2(self, current)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual artifact source differs from current original repository",
            ));
        }
        let raw = self.api(
            &format!(
                "repos/{}/actions/artifacts/{}",
                current.repository().full_name,
                target.model.id
            ),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let (actual, actual_node) = artifact(&raw, &target.repository, &target.run)?;
        if actual != target.model || actual_node != target.node {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Original artifact metadata/node/digest/expiry changed",
            ));
        }
        Ok(())
    }
    pub fn scoped_actions_archive_v2(
        &self,
        current: &CurrentCatalogueRepository,
        target: Arc<ScopedActionsArtifactV2>,
        cancel: &CancellationToken,
        mut guard: impl FnMut() -> GithubResult<()>,
    ) -> GithubResult<ScopedActionsResourceReadV2> {
        self.scoped_resource_run_current_v2(current, &target.run, cancel)?;
        self.scoped_artifact_current_v2(current, &target, cancel)?;
        if target.model.expired {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual selected artifact is expired; no private download requested",
            ));
        }
        guard()?;
        end(cancel)?;
        // The Actions-only immutable pool and original provider window still
        // clamp this4MiB ceiling, including an original narrower512KiB caller.
        let limits = ArtifactLimits {
            compressed_bytes: 4 * 1024 * 1024,
            expanded_bytes: 16 * 1024 * 1024,
            entry_bytes: 4 * 1024 * 1024,
            preview_bytes: 256 * 1024,
            entries: 4096,
            compression_ratio: 200,
        };
        let archive = Arc::new(self.download_scoped_actions_artifact_v2(
            &current.repository().full_name,
            &target.model,
            limits,
            cancel,
        )?);
        self.scoped_artifact_current_v2(current, &target, cancel)?;
        self.scoped_resource_run_current_v2(current, &target.run, cancel)?;
        end(cancel)?;
        Ok(ScopedActionsResourceReadV2 {
            repository: ScopedActionsRepositoryV2::from_current(self, current)?,
            body: ScopedActionsResourceBodyV2::Artifact { target, archive },
        })
    }
    pub fn scoped_actions_workflows_v2(
        &self,
        current: &CurrentCatalogueRepository,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedActionsResourceReadV2> {
        let repository = ScopedActionsRepositoryV2::from_current(self, current)?;
        let raw = self.api_list(
            &format!("repos/{}/actions/workflows?per_page=100", repository.name),
            Some("workflows"),
            cancel,
        )?;
        bound_rows(&raw)?;
        let mut ids = BTreeSet::new();
        let mut targets = Vec::with_capacity(raw.len());
        for row in &raw {
            let model = super::workflows::workflow_from_json_v2(row);
            super::workflows::validate_workflow_v2(&model)?;
            if !ids.insert(model.id) {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Duplicate workflow numeric identity",
                ));
            }
            targets.push(Arc::new(ScopedActionsWorkflowV2 {
                repository: repository.clone(),
                node: node(row)?,
                model,
            }));
        }
        end(cancel)?;
        Ok(ScopedActionsResourceReadV2 {
            repository,
            body: ScopedActionsResourceBodyV2::Workflows(targets.into()),
        })
    }
    pub fn scoped_actions_definition_v2(
        &self,
        current: &CurrentCatalogueRepository,
        target: Arc<ScopedActionsWorkflowV2>,
        revision: &str,
        cancel: &CancellationToken,
        mut guard: impl FnMut() -> GithubResult<()>,
    ) -> GithubResult<ScopedActionsResourceReadV2> {
        if !target.repository.matches(self, current)
            || revision.is_empty()
            || revision.len() > 4096
            || revision.chars().any(char::is_control)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Original workflow proof and bounded explicit revision required",
            ));
        }
        let raw = self.api(
            &format!(
                "repos/{}/actions/workflows/{}",
                current.repository().full_name,
                target.model.id
            ),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let actual = super::workflows::workflow_from_json_v2(&raw);
        super::workflows::validate_workflow_v2(&actual)?;
        if actual.id != target.model.id
            || actual.path != target.model.path
            || node(&raw)? != target.node
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original workflow numeric ID/node/path changed",
            ));
        }
        guard()?;
        end(cancel)?;
        let definition = Arc::new(self.workflow_definition(
            &current.repository().full_name,
            &actual,
            revision,
            cancel,
        )?);
        let closing = self.api(
            &format!(
                "repos/{}/actions/workflows/{}",
                current.repository().full_name,
                target.model.id
            ),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let closing_model = super::workflows::workflow_from_json_v2(&closing);
        super::workflows::validate_workflow_v2(&closing_model)?;
        if closing_model.id != actual.id
            || closing_model.path != actual.path
            || node(&closing)? != target.node
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Workflow ID/node/path changed during original YAML read",
            ));
        }
        end(cancel)?;
        Ok(ScopedActionsResourceReadV2 {
            repository: ScopedActionsRepositoryV2::from_current(self, current)?,
            body: ScopedActionsResourceBodyV2::Definition { target, definition },
        })
    }
    pub fn scoped_actions_repository_settings_v2(
        &self,
        current: &CurrentCatalogueRepository,
        kind: ScopedActionsRepositoryReadV2,
        cancel: &CancellationToken,
        mut guard: impl FnMut() -> GithubResult<()>,
    ) -> GithubResult<ScopedActionsResourceReadV2> {
        let repository = ScopedActionsRepositoryV2::from_current(self, current)?;
        let scope = ActionsScope::Repository {
            repository: repository.name.clone(),
        };
        guard()?;
        end(cancel)?;
        let body: GithubResult<ScopedActionsResourceBodyV2> = (|| {
            Ok(match kind {
                ScopedActionsRepositoryReadV2::Secrets => {
                    let rows = self.secret_metadata(&scope, cancel)?;
                    bound_rows(&rows)?;
                    ScopedActionsResourceBodyV2::Secrets(rows)
                }
                ScopedActionsRepositoryReadV2::Variables => {
                    let rows = self.variables(&scope, cancel)?;
                    bound_rows(&rows)?;
                    ScopedActionsResourceBodyV2::Variables(rows)
                }
                ScopedActionsRepositoryReadV2::Policy(section) => {
                    ScopedActionsResourceBodyV2::Policy(Box::new(
                        self.actions_policy(&scope, section, cancel)?,
                    ))
                }
                ScopedActionsRepositoryReadV2::Environments => {
                    let rows = self.environments(&repository.name, cancel)?;
                    bound_rows(&rows)?;
                    ScopedActionsResourceBodyV2::Environments(rows)
                }
                ScopedActionsRepositoryReadV2::Runners => {
                    let rows = self.runners(&scope, cancel)?;
                    bound_rows(&rows)?;
                    ScopedActionsResourceBodyV2::Runners(rows)
                }
                ScopedActionsRepositoryReadV2::Caches => {
                    let entries = self.action_caches(&repository.name, None, None, cancel)?;
                    bound_rows(&entries)?;
                    let usage = match self.cache_usage(&repository.name, cancel) {
                        Ok(value) => Some(value),
                        Err(e) if unavailable(&e) => None,
                        Err(e) => return Err(e),
                    };
                    ScopedActionsResourceBodyV2::Caches { entries, usage }
                }
            })
        })();
        let body=match body{Ok(body)=>body,Err(e)if unavailable(&e)=>ScopedActionsResourceBodyV2::Unavailable("This actual settings read is unavailable on the selected host/account; no administrator permission or empty inventory inferred"),Err(e)=>return Err(e)};
        end(cancel)?;
        Ok(ScopedActionsResourceReadV2 { repository, body })
    }
}
