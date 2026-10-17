//! Prepared native Actions pages. Provider and archive I/O happens only in workers.
use crate::{
    providers::{gh::*, runner::CancellationToken},
    storage::now,
    theme::SemanticToken,
};
use serde_json::Value;
use std::sync::Arc;
mod artifact;
mod commands;
pub(crate) mod scoped_read_v1;
pub(crate) mod scoped_resources_v2;
pub use commands::*;
mod approvals;
mod publication_targets;
mod publications;
mod settings;
pub use publications::*;
mod lease_snapshot;
pub use lease_snapshot::ActionsPublicationLeaseSnapshot;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActionsView {
    #[default]
    Runs,
    Workflows,
    Run,
    Jobs,
    Logs,
    Annotations,
    Artifacts,
    Artifact,
    Workflow,
    Approvals,
    Caches,
    Secrets,
    Variables,
    Environments,
    Policies,
    Runners,
    RunnerGroups,
    Usage,
    Reconciliation,
    Publications,
    PublicationInspection,
}
impl ActionsView {
    pub const CATALOGS: [(Self, &'static str); 11] = [
        (Self::Runs, "Runs"),
        (Self::Workflows, "Workflows"),
        (Self::Caches, "Caches"),
        (Self::Secrets, "Secrets"),
        (Self::Variables, "Variables"),
        (Self::Environments, "Environments"),
        (Self::Policies, "Policies"),
        (Self::Runners, "Runners"),
        (Self::RunnerGroups, "Runner groups"),
        (Self::Usage, "Usage"),
        (Self::Approvals, "Approvals"),
    ];
    pub fn label(self) -> &'static str {
        Self::CATALOGS
            .iter()
            .find(|(view, _)| *view == self)
            .map_or(
                match self {
                    Self::Run => "Run",
                    Self::Jobs => "Jobs and steps",
                    Self::Logs => "Logs",
                    Self::Annotations => "Annotations",
                    Self::Artifacts => "Artifacts",
                    Self::Artifact => "Archive preview",
                    Self::Workflow => "Workflow definition",
                    Self::Reconciliation => "Inspect uncertain write",
                    Self::Publications => "Original Actions publications",
                    Self::PublicationInspection => "Inspect original Actions publication",
                    _ => "Actions",
                },
                |(_, label)| *label,
            )
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionsAction {
    EditWorkflowLocalV9,
    ScopedReadV2(crate::scoped_actions_resources_v2::ScopedActionsReadActionV2),
    View(ActionsView),
    Repository(usize),
    Scope(ActionsScope),
    Select(usize),
    ControlsPage(i32),
    OpenRun {
        repository: String,
        id: u64,
        attempt: u64,
    },
    OpenJob(u64),
    Logs(Option<u64>),
    Annotations(u64),
    Artifacts,
    OpenArtifact(u64),
    ArtifactEntry(usize),
    OpenWorkflow {
        id: u64,
        revision: String,
    },
    Policy(PolicySection),
    Back,
    Refresh,
    Watch,
    Compose(ActionsCommand),
    /// Inspect an unavailable control without preparing or executing its mutation.
    Unavailable(String),
    Reconcile,
    Acknowledge,
    /// Worker handles SQL and positive-account checks; no effect is resent.
    PublicationList,
    PublicationInspect(i64),
    PublicationAcknowledge(i64),
    PublicationAcknowledgeAcceptance(i64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionsCommand {
    ChooseScope,
    WorkflowRevision,
    SearchLogs,
    FilterRuns,
    FilterCaches,
    RerunAll,
    RerunFailed,
    RerunJob,
    Cancel,
    ForceCancel,
    DeleteRun,
    DeleteLogs,
    ApproveFork,
    EnableWorkflow,
    DisableWorkflow,
    DispatchWorkflow,
    ExportArtifact,
    ExtractArtifact,
    DeleteArtifact,
    ApproveDeployment,
    RejectDeployment,
    DeleteCache,
    SetSecret,
    DeleteSecret,
    SetVariable,
    DeleteVariable,
    EditPolicy,
    EditEnvironment,
    RunnerLabels,
    RemoveRunner,
    EditRunnerGroup,
    GroupRunners,
    GroupRepositories,
    CreateRunnerGroup,
    DeleteRunnerGroup,
}
impl ActionsCommand {
    pub fn label(self) -> &'static str {
        use ActionsCommand::*;
        match self {
            ChooseScope => "Choose Actions scope",
            WorkflowRevision => "Choose workflow revision",
            SearchLogs => "Search current logs",
            FilterRuns => "Filter runs",
            FilterCaches => "Filter caches",
            RerunAll => "Rerun all jobs",
            RerunFailed => "Rerun failed jobs",
            RerunJob => "Rerun selected job",
            Cancel => "Cancel run",
            ForceCancel => "Force-cancel run",
            DeleteRun => "Delete run",
            DeleteLogs => "Delete run logs",
            ApproveFork => "Approve fork run",
            EnableWorkflow => "Enable workflow",
            DisableWorkflow => "Disable workflow",
            DispatchWorkflow => "Dispatch workflow",
            ExportArtifact => "Export artifact ZIP",
            ExtractArtifact => "Extract artifact into a new directory",
            DeleteArtifact => "Delete artifact",
            ApproveDeployment => "Approve selected deployment",
            RejectDeployment => "Reject selected deployment",
            DeleteCache => "Delete selected cache",
            SetSecret => "Set secret",
            DeleteSecret => "Delete secret",
            SetVariable => "Set variable",
            DeleteVariable => "Delete variable",
            EditPolicy => "Edit selected Actions policy",
            EditEnvironment => "Edit selected environment protection",
            RunnerLabels => "Replace custom runner labels",
            RemoveRunner => "Remove selected runner",
            EditRunnerGroup => "Edit selected runner group",
            GroupRunners => "Replace group runners",
            GroupRepositories => "Replace group repositories",
            CreateRunnerGroup => "Create runner group",
            DeleteRunnerGroup => "Delete runner group",
        }
    }
}
#[derive(Clone, Debug)]
pub struct ActionsRow {
    pub label: String,
    pub role: SemanticToken,
    pub actor: Option<Actor>,
    pub action: Option<ActionsAction>,
}
impl ActionsRow {
    fn text(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            role: SemanticToken::Normal,
            actor: None,
            action: None,
        }
    }
    fn clickable(label: impl Into<String>, action: ActionsAction) -> Self {
        Self {
            action: Some(action),
            ..Self::text(label)
        }
    }
}
#[derive(Clone, Debug)]
pub struct ActionsControl {
    pub label: String,
    pub action: ActionsAction,
    pub disabled_reason: Option<String>,
}
#[derive(Clone, Debug)]
pub struct ScopedRun {
    pub repository: String,
    pub run: WorkflowRun,
}
#[derive(Clone, Debug)]
pub struct RunPage {
    pub repository: String,
    pub attempt: RunAttempt,
    pub selected_job: Option<u64>,
    pub logs: Option<String>,
    pub annotations: Vec<CheckAnnotation>,
}
#[derive(Clone, Debug)]
pub struct ActionsWorkspace {
    pub identity: HostIdentity,
    pub repositories: Vec<String>,
    pub selected_repository: usize,
    pub scope: ActionsScope,
    pub view: ActionsView,
    pub filter: RunFilter,
    pub runs: Vec<ScopedRun>,
    pub workflows: Vec<Workflow>,
    default_workflow_revision: Option<String>,
    pub coverage: Coverage,
    pub run: Option<RunPage>,
    pub definition: Option<WorkflowDefinition>,
    pub artifacts: Vec<ActionsArtifact>,
    pub artifact: Option<ActionsArtifact>,
    pub archive: Option<Arc<ArtifactArchive>>,
    pub artifact_preview: Option<String>,
    artifact_image: Option<Arc<crate::identity::ImageDisplay>>,
    artifact_image_rows: Option<(usize, usize)>,
    pub deployments: Vec<PendingDeployment>,
    approval_history: Vec<DeploymentReviewRecord>,
    approval_history_error: Option<String>,
    accepted_deployments: std::collections::BTreeSet<u64>,
    gate_rows: Vec<Option<u64>>,
    pub caches: Vec<ActionCache>,
    cache_key: Option<String>,
    cache_ref: Option<String>,
    cache_usage: Option<CacheUsage>,
    pub secrets: Vec<SecretMetadata>,
    pub variables: Vec<ActionsVariable>,
    pub environments: Vec<Value>,
    environment_rows: Vec<Option<u64>>,
    pub policy: Option<PolicySnapshot>,
    pub policy_section: PolicySection,
    pub runners: Vec<SelfHostedRunner>,
    pub runner_groups: Vec<RunnerGroup>,
    pub rows: Vec<ActionsRow>,
    pub controls: Vec<ActionsControl>,
    pub selected: usize,
    pub top: usize,
    pub controls_page: usize,
    pub fetched_at: Option<u64>,
    pub warnings: Vec<String>,
    pub watching: bool,
    /// Plain outcome of the last watch tick, e.g. that the watched run finished.
    pub watch_note: Option<String>,
    pub log_search: Option<(String, bool)>,
    pub reconciliation_target: Option<ActionsTarget>,
    pub observation: Option<ActionsObservation>,
    pub publication_rows: Vec<ActionsPublicationRow>,
    pub publication_total: Option<u64>,
    pub publication_complete: bool,
    /// Display-only actual closed W9 rows; never source/permission or a target.
    pub recorded_workflow_history_v9: bool,
    pub publication_inspection: Option<ActionsPublicationInspectionPage>,
    pub publication_leases: Option<Arc<ActionsPublicationLeaseSnapshot>>,
}
/// Archives above this compressed size are not downloaded into memory for preview/export.
pub const ARTIFACT_IN_APP_LIMIT: u64 = 16 * 1024 * 1024;
fn artifact_too_large(artifact: &ActionsArtifact) -> bool {
    artifact.size_in_bytes > ARTIFACT_IN_APP_LIMIT
}
/// Human-readable binary size, e.g. "57.7 MiB".
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
pub fn status_role(status: &str, conclusion: &str) -> SemanticToken {
    match conclusion {
        "success" => SemanticToken::Success,
        "failure" | "timed_out" | "startup_failure" | "action_required" => SemanticToken::Failure,
        "cancelled" => SemanticToken::Cancelled,
        "skipped" | "neutral" | "stale" => SemanticToken::Muted,
        _ => match status {
            "in_progress" => SemanticToken::Running,
            "queued" | "waiting" | "pending" | "requested" => SemanticToken::Pending,
            _ => SemanticToken::Muted,
        },
    }
}
fn plain_error(message: impl Into<String>) -> GithubError {
    GithubError {
        kind: GithubErrorKind::InvalidTarget,
        message: message.into(),
    }
}
impl ActionsWorkspace {
    pub fn new(identity: HostIdentity, mut repositories: Vec<String>) -> Self {
        repositories.sort();
        repositories.dedup();
        let scope = ActionsScope::Repository {
            repository: repositories.first().cloned().unwrap_or_default(),
        };
        Self {
            identity,
            repositories,
            selected_repository: 0,
            scope,
            view: ActionsView::Runs,
            filter: RunFilter::default(),
            runs: vec![],
            workflows: vec![],
            default_workflow_revision: None,
            coverage: Coverage::default(),
            run: None,
            definition: None,
            artifacts: vec![],
            artifact: None,
            archive: None,
            artifact_preview: None,
            artifact_image: None,
            artifact_image_rows: None,
            deployments: vec![],
            approval_history: vec![],
            approval_history_error: None,
            accepted_deployments: Default::default(),
            gate_rows: vec![],
            caches: vec![],
            cache_key: None,
            cache_ref: None,
            cache_usage: None,
            secrets: vec![],
            variables: vec![],
            environments: vec![],
            environment_rows: vec![],
            policy: None,
            policy_section: PolicySection::Permissions,
            runners: vec![],
            runner_groups: vec![],
            rows: vec![],
            controls: vec![],
            selected: 0,
            top: 0,
            controls_page: 0,
            fetched_at: None,
            warnings: vec![],
            watching: false,
            watch_note: None,
            log_search: None,
            reconciliation_target: None,
            observation: None,
            publication_rows: vec![],
            publication_total: None,
            publication_complete: false,
            recorded_workflow_history_v9: false,
            publication_inspection: None,
            publication_leases: None,
        }
    }
    pub fn repository(&self) -> GithubResult<&str> {
        self.repositories
            .get(self.selected_repository)
            .map(String::as_str)
            .ok_or_else(|| {
                plain_error(
                    "Choose a repository set or open an exact PR/commit Actions indicator first",
                )
            })
    }
    pub fn context(&self) -> String {
        let repo = self.repository().unwrap_or("no repository selected");
        let scope = match &self.scope {
            ActionsScope::Repository { repository } => format!("repository {repository}"),
            ActionsScope::Organization { organization } => format!("organization {organization}"),
            ActionsScope::Environment {
                repository,
                environment,
            } => {
                format!("environment {repository}/{environment}")
            }
        };
        let base = format!(
            "{} / @{} / {repo} / {} / {scope}",
            self.identity.host,
            self.identity.login,
            self.view.label()
        );
        self.run
            .as_ref()
            .filter(|_| {
                matches!(
                    self.view,
                    ActionsView::Run
                        | ActionsView::Jobs
                        | ActionsView::Logs
                        | ActionsView::Annotations
                        | ActionsView::Artifacts
                        | ActionsView::Artifact
                        | ActionsView::Approvals
                )
            })
            .map_or(base.clone(), |page| {
                format!(
                    "{base} / run {} attempt {} / {}",
                    page.attempt.run.id, page.attempt.run.attempt, page.attempt.run.head_sha
                )
            })
    }
    pub fn actors(&self) -> Vec<Actor> {
        crate::actor_avatar_projection_v1::actions(self)
            .map(crate::actor_avatar_projection_v1::distinct)
            .unwrap_or_default()
    }
    /// Watch refreshes only the run list or an open run (and its jobs).
    pub fn watch_applies(&self) -> bool {
        match self.view {
            ActionsView::Runs => true,
            ActionsView::Run | ActionsView::Jobs => self.run.is_some(),
            _ => false,
        }
    }
    /// Start or stop watching. Pure local state; the main loop performs the refreshes.
    pub fn toggle_watch(&mut self) -> Result<(), String> {
        if self.watching {
            self.watching = false;
            self.watch_note = Some("Watch stopped".into());
        } else if !self.watch_applies() {
            return Err(
                "Watch updates the run list or an open run: open Runs or a run, then press w"
                    .into(),
            );
        } else {
            self.watching = true;
            self.watch_note = None;
        }
        let label = self.watch_label();
        for control in &mut self.controls {
            if control.action == ActionsAction::Watch {
                control.label = label.into();
            }
        }
        Ok(())
    }
    fn watch_label(&self) -> &'static str {
        if self.watching {
            "[w Stop watch]"
        } else {
            "[w Watch]"
        }
    }
    /// One watch tick observed `run`: stop once it has finished, otherwise report progress.
    pub fn observe_watched_run(&mut self, run: &WorkflowRun) {
        if run.status == "completed" {
            self.watching = false;
            self.watch_note = Some(format!(
                "Watch ended: run {} attempt {} finished • {}",
                run.id,
                run.attempt,
                if run.conclusion.is_empty() {
                    "no conclusion reported"
                } else {
                    &run.conclusion
                }
            ));
        } else {
            self.watch_note = Some(format!(
                "Run {} attempt {} is {}",
                run.id,
                run.attempt,
                run.status.replace('_', " ")
            ));
        }
    }
    pub fn select(&mut self, index: usize) {
        self.selected = index.min(self.rows.len().saturating_sub(1));
        for index in 0..self.controls.len() {
            if let ActionsAction::Compose(command) = self.controls[index].action {
                self.controls[index].disabled_reason = self.disabled_reason(command);
            }
        }
    }
    fn selected_deployment(&self) -> Option<&PendingDeployment> {
        if self.view != ActionsView::Approvals {
            return None;
        }
        let id = self.gate_rows.get(self.selected).copied().flatten()?;
        self.deployments
            .iter()
            .find(|gate| gate.environment_id == id)
    }
    fn selected_environment(&self) -> Option<&Value> {
        if self.view != ActionsView::Environments {
            return None;
        }
        let id = self
            .environment_rows
            .get(self.selected)
            .copied()
            .flatten()?;
        self.environments
            .iter()
            .find(|environment| environment["id"].as_u64() == Some(id))
    }
    pub(crate) fn artifact_image(&self) -> Option<(&crate::identity::ImageDisplay, usize, usize)> {
        let (start, count) = self.artifact_image_rows?;
        Some((self.artifact_image.as_deref()?, start, count))
    }
    pub fn mark_uncertain(&mut self, target: Option<ActionsTarget>, reason: impl Into<String>) {
        self.reconciliation_target = target;
        self.observation = None;
        self.warnings.push(reason.into());
        self.prepare_rows();
    }
    pub fn selected_action(&self) -> Option<ActionsAction> {
        self.rows.get(self.selected)?.action.clone()
    }
    pub fn load_catalog(
        &mut self,
        gh: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.runs.clear();
        self.coverage = Coverage {
            complete: true,
            ..Coverage::default()
        };
        self.warnings.clear();
        if self.repositories.is_empty() {
            self.coverage.complete = false;
            self.coverage.reasons.push("No repository scope selected; map a local checkout or choose a repository set before Refresh".into());
            self.view = ActionsView::Runs;
            self.prepare_rows();
            return Ok(());
        }
        if self.repositories.len() > 64 {
            return Err(plain_error(
                "Actions catalog is limited to 64 explicitly selected repositories per refresh",
            ));
        }
        for repository in &self.repositories {
            if cancel.is_cancelled() {
                self.coverage.complete = false;
                self.coverage.reasons.push(
                    "Refresh cancelled; previously loaded repositories may be incomplete".into(),
                );
                break;
            }
            match gh.run_catalog(repository, &self.filter, cancel) {
                Ok(catalog) => {
                    self.coverage.total_matches += catalog.coverage.total_matches;
                    self.coverage.complete &= catalog.coverage.complete;
                    self.coverage.reasons.extend(
                        catalog
                            .coverage
                            .reasons
                            .into_iter()
                            .map(|reason| format!("{repository}: {reason}")),
                    );
                    self.runs
                        .extend(catalog.runs.into_iter().map(|run| ScopedRun {
                            repository: repository.clone(),
                            run,
                        }));
                }
                Err(error) => {
                    self.coverage.complete = false;
                    self.coverage
                        .reasons
                        .push(format!("{repository}: {}", error.message));
                }
            }
        }
        self.runs.sort_by(|a, b| {
            b.run
                .created_at
                .cmp(&a.run.created_at)
                .then_with(|| b.run.id.cmp(&a.run.id))
        });
        self.coverage.loaded = self.runs.len();
        self.fetched_at = Some(now());
        self.view = ActionsView::Runs;
        self.prepare_rows();
        Ok(())
    }
    /// Run in a worker. Select, Watch and Acknowledge themselves have no external effects.
    pub fn apply(
        &mut self,
        gh: &GithubProvider,
        action: &ActionsAction,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        use ActionsAction::*;
        if matches!(
            action,
            OpenRun { .. } | View(_) | Back | Repository(_) | Artifacts | Logs(_)
        ) {
            self.watch_note = None;
        }
        match action {
            EditWorkflowLocalV9 => return Err(plain_error("Local workflow editing requires the actual original Definition/Actions worker; no legacy mapping or write fallback")),
            ScopedReadV2(_) => {
                return Err(plain_error(
                    "This read needs its actual original-scoped Actions owner; no legacy fallback",
                ))
            }
            Select(index) => {
                self.select(*index);
                return Ok(());
            }
            ControlsPage(delta) => {
                self.controls_page = self.controls_page.saturating_add_signed(*delta as isize);
                return Ok(());
            }
            Watch => {
                if let Err(reason) = self.toggle_watch() {
                    self.warnings.push(reason);
                }
                self.prepare_rows();
                return Ok(());
            }
            Compose(_) => return Ok(()),
            PublicationList
            | PublicationInspect(_)
            | PublicationAcknowledge(_)
            | PublicationAcknowledgeAcceptance(_) => {
                return Err(plain_error(
                    "Durable publication actions require the verified worker route; nothing sent",
                ));
            }
            Unavailable(reason) => {
                self.warnings.push(reason.clone());
                self.prepare_rows();
                return Ok(());
            }
            Repository(index) => {
                if *index >= self.repositories.len() {
                    return Err(plain_error("Selected repository is unavailable"));
                }
                self.selected_repository = *index;
                self.scope = ActionsScope::Repository {
                    repository: self.repository()?.into(),
                };
                self.run = None;
                self.deployments.clear();
                self.approval_history.clear();
                self.approval_history_error = None;
                self.accepted_deployments.clear();
                self.definition = None;
                self.workflows.clear();
                self.default_workflow_revision = None;
                self.artifact = None;
                self.archive = None;
                self.artifact_preview = None;
                self.artifact_image = None;
                self.caches.clear();
                self.cache_key = None;
                self.cache_ref = None;
                self.cache_usage = None;
                self.view = ActionsView::Runs;
                return self.load_catalog(gh, cancel);
            }
            Scope(scope) => {
                self.scope = scope.clone();
                self.view = ActionsView::Secrets;
            }
            View(view) => {
                self.view = *view;
                self.selected = 0;
                self.top = 0;
                self.controls_page = 0;
            }
            OpenRun {
                repository,
                id,
                attempt,
            } => {
                let index = self
                    .repositories
                    .iter()
                    .position(|repo| repo == repository)
                    .ok_or_else(|| plain_error("Run repository is outside the selected scope"))?;
                if index != self.selected_repository {
                    self.workflows.clear();
                    self.definition = None;
                    self.default_workflow_revision = None;
                }
                self.selected_repository = index;
                let same_run = self.run.as_ref().is_some_and(|page| {
                    page.repository.eq_ignore_ascii_case(repository)
                        && page.attempt.run.id == *id
                        && page.attempt.run.attempt == *attempt
                });
                self.deployments.clear();
                self.approval_history.clear();
                self.approval_history_error = None;
                if !same_run {
                    self.accepted_deployments.clear();
                }
                self.run = Some(RunPage {
                    repository: repository.clone(),
                    attempt: gh.run_attempt(repository, *id, *attempt, cancel)?,
                    selected_job: None,
                    logs: None,
                    annotations: vec![],
                });
                self.view = ActionsView::Run;
                self.selected = 0;
                self.top = 0;
            }
            OpenJob(id) => {
                let page = self
                    .run
                    .as_mut()
                    .ok_or_else(|| plain_error("Open an exact run attempt first"))?;
                if !page.attempt.jobs.iter().any(|job| job.id == *id) {
                    return Err(plain_error("Job is not in the selected attempt"));
                }
                page.selected_job = Some(*id);
                self.view = ActionsView::Jobs;
                self.selected = 0;
                self.top = 0;
            }
            Logs(id) => {
                let page = self
                    .run
                    .as_mut()
                    .ok_or_else(|| plain_error("Open an exact run attempt first"))?;
                let job = id
                    .map(|id| {
                        page.attempt
                            .jobs
                            .iter()
                            .find(|job| job.id == id)
                            .ok_or_else(|| plain_error("Job belongs to another attempt"))
                    })
                    .transpose()?;
                page.logs =
                    Some(gh.run_logs_at(&page.repository, &page.attempt.run, job, cancel)?);
                page.selected_job = *id;
                self.view = ActionsView::Logs;
                self.selected = 0;
                self.top = 0;
            }
            Annotations(id) => {
                let page = self
                    .run
                    .as_mut()
                    .ok_or_else(|| plain_error("Open a run first"))?;
                let job = page
                    .attempt
                    .jobs
                    .iter()
                    .find(|job| job.id == *id)
                    .ok_or_else(|| plain_error("Job belongs to another attempt"))?;
                page.annotations = gh.job_annotations(&page.repository, job, cancel)?;
                page.selected_job = Some(*id);
                self.view = ActionsView::Annotations;
                self.selected = 0;
                self.top = 0;
            }
            Artifacts => {
                self.view = ActionsView::Artifacts;
            }
            OpenArtifact(id) => {
                let artifact = self
                    .artifacts
                    .iter()
                    .find(|artifact| artifact.id == *id)
                    .cloned()
                    .ok_or_else(|| plain_error("Artifact is outside the displayed catalog"))?;
                // Larger archives are not downloaded into memory; the page still offers Delete
                // and explains how to download them outside ghtui.
                self.archive = if artifact_too_large(&artifact) {
                    None
                } else {
                    Some(Arc::new(gh.download_artifact(
                        self.repository()?,
                        &artifact,
                        ArtifactLimits::default(),
                        cancel,
                    )?))
                };
                self.artifact = Some(artifact);
                self.artifact_preview = None;
                self.artifact_image = None;
                self.view = ActionsView::Artifact;
                self.selected = 0;
                self.top = 0;
            }
            ArtifactEntry(index) => {
                let archive = self
                    .archive
                    .as_ref()
                    .ok_or_else(|| plain_error("Download and inspect an artifact first"))?;
                let entry = archive
                    .entries
                    .get(*index)
                    .ok_or_else(|| plain_error("Archive entry disappeared"))?;
                let content = artifact::prepare_content(archive, &entry.path, cancel)?;
                self.artifact_preview = Some(content.text);
                self.artifact_image = content.image;
                self.top = 0;
            }
            OpenWorkflow { id, revision } => {
                let workflow = self
                    .workflows
                    .iter()
                    .find(|workflow| workflow.id == *id)
                    .cloned()
                    .ok_or_else(|| {
                        plain_error("Select a workflow from this repository catalog first")
                    })?;
                self.definition = Some(gh.workflow_definition(
                    self.repository()?,
                    &workflow,
                    revision,
                    cancel,
                )?);
                self.view = ActionsView::Workflow;
                self.selected = 0;
                self.top = 0;
            }
            Policy(section) => {
                self.policy_section = *section;
                self.view = ActionsView::Policies;
            }
            Back => {
                self.view = if matches!(
                    self.view,
                    ActionsView::Jobs
                        | ActionsView::Logs
                        | ActionsView::Annotations
                        | ActionsView::Artifacts
                        | ActionsView::Approvals
                ) {
                    if self.run.is_some() {
                        ActionsView::Run
                    } else {
                        ActionsView::Runs
                    }
                } else if self.view == ActionsView::Artifact {
                    ActionsView::Artifacts
                } else {
                    ActionsView::Runs
                };
                self.selected = 0;
                self.top = 0;
            }
            Refresh => {}
            Reconcile => {
                let target = self
                    .reconciliation_target
                    .as_ref()
                    .ok_or_else(|| plain_error("No uncertain mutation target to inspect"))?;
                self.observation =
                    Some(gh.inspect_actions_target(&self.identity, target, cancel)?);
                self.view = ActionsView::Reconciliation;
            }
            Acknowledge => {
                let observation = self
                    .observation
                    .take()
                    .ok_or_else(|| plain_error("Read the affected state before acknowledging"))?;
                gh.acknowledge_actions_observation(observation)?;
                self.reconciliation_target = None;
                self.load_catalog(gh, cancel)?;
                self.warnings
                    .push("Affected state acknowledged; no mutation was repeated".into());
                // Catalogue refresh clears old warnings. Add the acknowledgement only
                // afterwards so the confirmed recovery remains visibly inspectable.
                self.prepare_rows();
                return Ok(());
            }
        }
        if !matches!(action, OpenRun { .. } | OpenJob(_)) {
            self.load_view(gh, cancel)?;
        }
        self.fetched_at = Some(now());
        self.prepare_rows();
        Ok(())
    }
    fn load_view(&mut self, gh: &GithubProvider, cancel: &CancellationToken) -> GithubResult<()> {
        use ActionsView::*;
        match self.view {
            Runs => {
                // Keep the same run selected when a refresh or watch tick adds newer runs.
                let selected = self
                    .runs
                    .get(self.selected)
                    .map(|item| (item.repository.clone(), item.run.id));
                self.load_catalog(gh, cancel)?;
                if let Some(index) = selected.and_then(|(repository, id)| {
                    self.runs
                        .iter()
                        .position(|item| item.repository == repository && item.run.id == id)
                }) {
                    self.selected = index;
                }
                return Ok(());
            }
            Workflows => {
                let repo = gh.repository(self.repository()?, cancel)?;
                if repo.default_branch.is_empty() {
                    return Err(plain_error("The repository did not report an actual default branch; choose an explicit workflow revision"));
                }
                self.default_workflow_revision = Some(repo.default_branch);
                self.workflows = gh.workflows(self.repository()?, cancel)?;
            }
            Caches => {
                self.caches = gh.action_caches(
                    self.repository()?,
                    self.cache_key.as_deref(),
                    self.cache_ref.as_deref(),
                    cancel,
                )?;
                self.cache_usage = match gh.cache_usage(self.repository()?, cancel) {
                    Ok(usage) => Some(usage),
                    Err(error) => {
                        self.warnings
                            .push(format!("Cache usage unavailable: {}", error.message));
                        None
                    }
                };
            }
            Secrets => self.secrets = gh.secret_metadata(&self.scope, cancel)?,
            Variables => self.variables = gh.variables(&self.scope, cancel)?,
            Environments => self.environments = gh.environments(self.repository()?, cancel)?,
            Policies => {
                self.policy = Some(gh.actions_policy(&self.scope, self.policy_section, cancel)?)
            }
            Runners => self.runners = gh.runners(&self.scope, cancel)?,
            RunnerGroups => {
                let org = match &self.scope {
                    ActionsScope::Organization { organization } => organization.clone(),
                    _ => {
                        return Err(plain_error(
                            "Runner groups require an explicitly selected organization scope",
                        ))
                    }
                };
                self.runner_groups = gh.runner_groups(&org, cancel)?;
            }
            Artifacts => {
                self.artifacts = gh.artifacts(
                    self.repository()?,
                    self.run.as_ref().map(|run| run.attempt.run.id),
                    cancel,
                )?
            }
            Approvals => {
                let run = self
                    .run
                    .as_ref()
                    .ok_or_else(|| plain_error("Open a waiting run before deployment approvals"))?;
                self.deployments =
                    gh.pending_deployments(&run.repository, run.attempt.run.id, cancel)?;
                self.accepted_deployments.retain(|id| {
                    self.deployments
                        .iter()
                        .any(|gate| gate.environment_id == *id)
                });
                self.approval_history_error = None;
                self.approval_history =
                    match gh.deployment_review_history(&run.repository, run.attempt.run.id, cancel)
                    {
                        Ok(history) => history,
                        Err(error) => {
                            self.approval_history_error = Some(error.message.clone());
                            self.warnings.push(format!(
                                "Approval history unavailable on this host/account: {}",
                                error.message
                            ));
                            vec![]
                        }
                    };
            }
            Run | Jobs => {
                let page = self
                    .run
                    .as_mut()
                    .ok_or_else(|| plain_error("Open a run first"))?;
                // A normal refresh preserves the selected attempt; watching follows the current attempt.
                let attempt = if self.watching {
                    gh.run(&page.repository, page.attempt.run.id, cancel)?
                        .attempt
                } else {
                    page.attempt.run.attempt
                };
                page.attempt =
                    gh.run_attempt(&page.repository, page.attempt.run.id, attempt, cancel)?;
                if page
                    .selected_job
                    .is_some_and(|id| !page.attempt.jobs.iter().any(|job| job.id == id))
                {
                    page.selected_job = None;
                }
                if self.watching {
                    let run = self.run.as_ref().map(|page| page.attempt.run.clone());
                    if let Some(run) = run {
                        self.observe_watched_run(&run);
                    }
                }
            }
            Usage => {
                let usage = gh.cache_usage(self.repository()?, cancel)?;
                self.rows=vec![ActionsRow::text(format!("Cache storage: {} bytes in {} caches",usage.active_caches_size_in_bytes,usage.active_caches_count)),ActionsRow::text("Cache units are bytes. Workflow timing is milliseconds, not cost; billing multipliers are not inferred.")];
            }
            Workflow
            | Logs
            | Annotations
            | Artifact
            | Reconciliation
            | Publications
            | PublicationInspection => {}
        }
        Ok(())
    }
    pub fn prepare_rows(&mut self) {
        use ActionsView::*;
        let selected_environment = self
            .selected_environment()
            .and_then(|environment| environment["id"].as_u64());
        self.controls = vec![];
        self.artifact_image_rows = None;
        self.gate_rows.clear();
        self.environment_rows.clear();
        let add = |label: &str, action: ActionsAction| ActionsControl {
            label: label.into(),
            action,
            disabled_reason: None,
        };
        self.controls.extend(Self::catalog_controls());
        self.controls
            .push(add("[r Refresh]", ActionsAction::Refresh));
        self.controls
            .push(add("Publications", ActionsAction::PublicationList));
        if self.view != Runs {
            self.controls.push(add("[b Back]", ActionsAction::Back));
        }
        let command = |value: ActionsCommand| add(value.label(), ActionsAction::Compose(value));
        self.controls.push(command(ActionsCommand::ChooseScope));
        if matches!(self.view, Logs | Annotations) {
            self.controls.push(add(
                "[l Logs]",
                ActionsAction::Logs(self.run.as_ref().and_then(|page| page.selected_job)),
            ));
            if let Some(id) = self.run.as_ref().and_then(|page| page.selected_job) {
                self.controls
                    .push(add("[N Annotations]", ActionsAction::Annotations(id)));
            }
        }
        if self.view == Logs {
            self.controls.push(command(ActionsCommand::SearchLogs));
        }
        if self.view == Workflow {
            self.controls
                .push(command(ActionsCommand::WorkflowRevision));
        }
        if self.reconciliation_target.is_some() {
            self.controls
                .push(add("Inspect uncertain write", ActionsAction::Reconcile));
        }
        self.rows = match self.view {
            Runs => {
                self.controls.push(command(ActionsCommand::FilterRuns));
                self.controls
                    .push(add(self.watch_label(), ActionsAction::Watch));
                self.runs
                    .iter()
                    .map(|item| ActionsRow {
                        label: format!(
                            "{} run {} attempt {} • {} • {} / {} • {} • {}",
                            item.repository,
                            item.run.id,
                            item.run.attempt,
                            item.run.name,
                            item.run.status,
                            item.run.conclusion,
                            item.run.head_branch,
                            item.run.head_sha
                        ),
                        role: status_role(&item.run.status, &item.run.conclusion),
                        actor: Some(item.run.actor.clone()),
                        action: Some(ActionsAction::OpenRun {
                            repository: item.repository.clone(),
                            id: item.run.id,
                            attempt: item.run.attempt,
                        }),
                    })
                    .collect()
            }
            Workflows => self
                .workflows
                .iter()
                .map(|workflow| {
                    ActionsRow::clickable(
                        format!(
                            "{} • {} • {} • #{}",
                            workflow.name, workflow.state, workflow.path, workflow.id
                        ),
                        ActionsAction::OpenWorkflow {
                            id: workflow.id,
                            revision: self
                                .default_workflow_revision
                                .clone()
                                .unwrap_or("HEAD".into()),
                        },
                    )
                })
                .collect(),
            Run | Jobs => {
                self.controls.extend([
                    add("[j Jobs]", ActionsAction::View(Jobs)),
                    add(
                        "[l Logs]",
                        ActionsAction::Logs(self.run.as_ref().and_then(|run| run.selected_job)),
                    ),
                    add("[a Artifacts]", ActionsAction::Artifacts),
                    add("[G Gates]", ActionsAction::View(Approvals)),
                    add(self.watch_label(), ActionsAction::Watch),
                ]);
                self.controls.extend(
                    [
                        ActionsCommand::RerunAll,
                        ActionsCommand::RerunFailed,
                        ActionsCommand::RerunJob,
                        ActionsCommand::Cancel,
                        ActionsCommand::ForceCancel,
                        ActionsCommand::DeleteRun,
                        ActionsCommand::DeleteLogs,
                        ActionsCommand::ApproveFork,
                    ]
                    .into_iter()
                    .map(command),
                );
                let mut rows = vec![];
                if let Some(page) = &self.run {
                    let run = &page.attempt.run;
                    rows.push(ActionsRow::text(format!(
                        "Run {} • attempt {} • {} / {} • trigger {} • branch {}",
                        run.id, run.attempt, run.status, run.conclusion, run.event, run.head_branch
                    )));
                    rows.push(ActionsRow::text(format!(
                        "Exact commit {} • workflow {} • {} → {}",
                        run.head_sha, run.workflow_id, run.created_at, run.updated_at
                    )));
                    rows.push(ActionsRow {
                        actor: Some(run.actor.clone()),
                        ..ActionsRow::text("run actor")
                    });
                    rows.push(ActionsRow {
                        actor: Some(run.triggering_actor.clone()),
                        ..ActionsRow::text("triggering actor")
                    });
                    rows.push(ActionsRow::text(&page.attempt.definition_note));
                    for job in &page.attempt.jobs {
                        rows.push(ActionsRow {
                            label: format!(
                                "Job {} • {} • {} / {} • runner {} [{}]",
                                job.id,
                                job.name,
                                job.status,
                                job.conclusion,
                                job.runner_name,
                                job.labels.join(", ")
                            ),
                            role: status_role(&job.status, &job.conclusion),
                            actor: None,
                            action: Some(ActionsAction::OpenJob(job.id)),
                        });
                        if self.view == Jobs && page.selected_job == Some(job.id) {
                            self.controls
                                .push(add("[N Annotations]", ActionsAction::Annotations(job.id)));
                            for step in &job.steps {
                                rows.push(ActionsRow {
                                    label: format!(
                                        "  Step {} • {} • {} / {} • {} → {}",
                                        step.number,
                                        step.name,
                                        step.status,
                                        step.conclusion,
                                        step.started_at,
                                        step.completed_at
                                    ),
                                    role: status_role(&step.status, &step.conclusion),
                                    actor: None,
                                    action: Some(ActionsAction::Logs(Some(job.id))),
                                });
                            }
                        }
                    }
                }
                rows
            }
            Logs => self
                .run
                .as_ref()
                .and_then(|page| page.logs.as_ref())
                .map(|logs| logs.lines().map(ActionsRow::text).collect())
                .unwrap_or_default(),
            Annotations => self
                .run
                .as_ref()
                .map(|page| {
                    page.annotations
                        .iter()
                        .flat_map(|note| {
                            [
                                ActionsRow {
                                    label: format!(
                                        "{}:{}–{} • {} • {}",
                                        note.path,
                                        note.start_line,
                                        note.end_line,
                                        note.level,
                                        note.title
                                    ),
                                    role: if note.level == "failure" {
                                        SemanticToken::Failure
                                    } else {
                                        SemanticToken::Pending
                                    },
                                    actor: None,
                                    action: None,
                                },
                                ActionsRow::text(&note.message),
                            ]
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Artifacts => self
                .artifacts
                .iter()
                .map(|artifact| ActionsRow {
                    label: format!(
                        "{} #{} • {} • run {} • attempt {} • expires {}{}",
                        artifact.name,
                        artifact.id,
                        human_bytes(artifact.size_in_bytes),
                        artifact.run_id,
                        artifact
                            .attempt
                            .map_or("not exposed".into(), |value| value.to_string()),
                        artifact.expires_at,
                        if artifact.expired { " [expired]" } else { "" }
                    ),
                    role: if artifact.expired {
                        SemanticToken::Muted
                    } else {
                        SemanticToken::Normal
                    },
                    actor: None,
                    action: Some(if artifact.expired {
                        ActionsAction::Unavailable(format!(
                            "Artifact {} #{} expired at {}; GitHub cannot provide its archive",
                            artifact.name, artifact.id, artifact.expires_at
                        ))
                    } else {
                        ActionsAction::OpenArtifact(artifact.id)
                    }),
                })
                .collect(),
            Artifact => {
                self.controls.extend(
                    [
                        ActionsCommand::ExportArtifact,
                        ActionsCommand::ExtractArtifact,
                        ActionsCommand::DeleteArtifact,
                    ]
                    .into_iter()
                    .map(command),
                );
                let mut rows = self
                    .archive
                    .as_ref()
                    .map(|archive| {
                        archive
                            .entries
                            .iter()
                            .enumerate()
                            .map(|(index, entry)| {
                                ActionsRow::clickable(
                                    format!("{} • {} bytes", entry.path, entry.expanded_bytes),
                                    if entry.directory {ActionsAction::Unavailable(format!("{} is an archive directory. Its contained files are listed individually; choose a regular file to preview",entry.path))}else{ActionsAction::ArtifactEntry(index)},
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if let Some(artifact) = &self.artifact {
                    rows.push(ActionsRow::text(format!(
                        "Artifact {} #{} • run {} • {} bytes • expires {}",
                        artifact.name,
                        artifact.id,
                        artifact.run_id,
                        artifact.size_in_bytes,
                        artifact.expires_at
                    )));
                    if self.archive.is_none() && artifact_too_large(artifact) {
                        let repository = self.repository().unwrap_or_default();
                        rows.push(ActionsRow {
                            role: SemanticToken::Pending,
                            ..ActionsRow::text(format!(
                                "Not downloaded: {} is over ghtui's {} preview/export limit. Delete still works here.",
                                human_bytes(artifact.size_in_bytes),
                                human_bytes(ARTIFACT_IN_APP_LIMIT),
                            ))
                        });
                        rows.push(ActionsRow::text(format!(
                            "Download it in a shell: gh run download {} -R {} -n {}",
                            artifact.run_id, repository, artifact.name
                        )));
                    }
                }
                if let Some(archive) = &self.archive {
                    rows.push(ActionsRow::text(if archive.digest_verified {"Integrity: supported SHA256 digest verified against downloaded ZIP bytes"}else{"Integrity: no supported SHA256 digest was available; ZIP paths/sizes are still validated"}));
                }
                rows.push(ActionsRow::text("Archive paths, types, counts, sizes and compression ratio checked. Contents are never executed."));
                if let Some(preview) = &self.artifact_preview {
                    rows.extend(preview.lines().map(ActionsRow::text));
                }
                if let Some(image) = &self.artifact_image {
                    let count = image.raster.height.div_ceil(2) as usize;
                    self.artifact_image_rows = Some((rows.len(), count));
                    rows.extend((0..count).map(|_| ActionsRow::text("")));
                }
                rows
            }
            Workflow => {
                self.controls.push(ActionsControl {
                    label: "[E Edit YAML locally]".into(), action: ActionsAction::EditWorkflowLocalV9,
                    disabled_reason: Some("Requires an actual source-bound workflow Definition; no legacy/local-name fallback".into()),
                });
                self.controls.extend(
                    [
                        ActionsCommand::EnableWorkflow,
                        ActionsCommand::DisableWorkflow,
                        ActionsCommand::DispatchWorkflow,
                    ]
                    .into_iter()
                    .map(command),
                );
                let mut rows = vec![];
                if let Some(definition) = &self.definition {
                    rows.push(ActionsRow::text(format!(
                        "{} • revision {} • exact commit {}",
                        definition.workflow.name, definition.revision, definition.commit_sha
                    )));
                    rows.push(ActionsRow::text(format!(
                        "Triggers: {}",
                        definition.triggers.join(", ")
                    )));
                    for input in &definition.dispatch_inputs {
                        rows.push(ActionsRow::text(format!(
                            "Input {} • {:?} • required {} • {}",
                            input.name, input.input_type, input.required, input.description
                        )));
                    }
                    for job in &definition.jobs {
                        rows.push(ActionsRow::text(format!(
                            "Definition job {} • needs {} • matrix {} • reusable {}",
                            job.id,
                            job.needs.join(", "),
                            job.matrix,
                            job.reusable_workflow.as_deref().unwrap_or("none")
                        )));
                    }
                    rows.push(ActionsRow::text("YAML edits use the mapped local checkout and normal Git preview/commit/PR flow; no workflow is executed locally."));
                    rows.extend(
                        definition.yaml.lines().enumerate().map(|(index, line)| {
                            ActionsRow::text(format!("{:4} {line}", index + 1))
                        }),
                    );
                }
                rows
            }
            Approvals => {
                self.controls.extend(
                    [
                        ActionsCommand::ApproveDeployment,
                        ActionsCommand::RejectDeployment,
                    ]
                    .into_iter()
                    .map(command),
                );
                let (rows, identities) = approvals::rows(&self.deployments, &self.approval_history);
                self.gate_rows = identities;
                let mut rows = rows;
                if let Some(error) = &self.approval_history_error {
                    rows.push(ActionsRow::text(format!(
                        "Approval history unavailable: {error}"
                    )));
                    self.gate_rows.push(None);
                }
                rows
            }
            Caches => {
                self.controls.push(command(ActionsCommand::FilterCaches));
                self.controls.push(command(ActionsCommand::DeleteCache));
                let mut rows = self
                    .caches
                    .iter()
                    .map(|cache| {
                        ActionsRow::text(format!(
                            "#{} • {} • {} • {} bytes • last used {} • created {}",
                            cache.id,
                            cache.key,
                            cache.ref_name,
                            cache.size_in_bytes,
                            cache.last_accessed_at,
                            cache.created_at
                        ))
                    })
                    .collect::<Vec<_>>();
                rows.push(ActionsRow::text(format!(
                    "Cache filter: key {} • ref {} • {} displayed",
                    self.cache_key.as_deref().unwrap_or("all"),
                    self.cache_ref.as_deref().unwrap_or("all"),
                    self.caches.len()
                )));
                if let Some(usage) = &self.cache_usage {
                    rows.push(ActionsRow::text(format!(
                        "Repository cache usage: {} bytes in {} caches (not filtered)",
                        usage.active_caches_size_in_bytes, usage.active_caches_count
                    )));
                }
                rows
            }
            Secrets => {
                self.controls.extend(
                    [ActionsCommand::SetSecret, ActionsCommand::DeleteSecret]
                        .into_iter()
                        .map(command),
                );
                let mut rows=vec![ActionsRow::text("Existing secret values cannot be read. Only names and metadata are displayed; input values are transient and masked.")];
                rows.extend(self.secrets.iter().map(|secret| {
                    ActionsRow::text(format!(
                        "{} • updated {} • visibility {}",
                        secret.name,
                        secret.updated_at,
                        secret.visibility.as_deref().unwrap_or("scope inherited")
                    ))
                }));
                rows
            }
            Variables => {
                self.controls.extend(
                    [ActionsCommand::SetVariable, ActionsCommand::DeleteVariable]
                        .into_iter()
                        .map(command),
                );
                self.variables
                    .iter()
                    .map(|variable| {
                        ActionsRow::text(format!(
                            "{} = {} • updated {}",
                            variable.name, variable.value, variable.updated_at
                        ))
                    })
                    .collect()
            }
            Environments => {
                self.controls.push(command(ActionsCommand::EditEnvironment));
                let (rows, identities) = settings::environment_rows(&self.environments);
                self.environment_rows = identities;
                rows
            }
            Policies => {
                self.controls.push(command(ActionsCommand::EditPolicy));
                self.controls.extend(
                    [
                        (PolicySection::Permissions, "Allowlist"),
                        (PolicySection::SelectedActions, "Selected actions"),
                        (PolicySection::WorkflowToken, "Workflow token"),
                        (PolicySection::ForkWorkflows, "Fork workflow"),
                        (PolicySection::ForkApproval, "Fork approval"),
                        (PolicySection::Retention, "Retention"),
                    ]
                    .into_iter()
                    .map(|(section, label)| add(label, ActionsAction::Policy(section))),
                );
                self.policy
                    .as_ref()
                    .map(|snapshot| {
                        serde_json::to_string_pretty(&snapshot.value)
                            .unwrap_or_default()
                            .lines()
                            .map(ActionsRow::text)
                            .collect()
                    })
                    .unwrap_or_default()
            }
            Runners => {
                self.controls.extend(
                    [ActionsCommand::RunnerLabels, ActionsCommand::RemoveRunner]
                        .into_iter()
                        .map(command),
                );
                self.runners
                    .iter()
                    .map(|runner| {
                        ActionsRow::text(format!(
                            "#{} • {} • {} • busy {} • OS {} • labels {}",
                            runner.id,
                            runner.name,
                            runner.status,
                            runner.busy,
                            runner.os,
                            runner
                                .labels
                                .iter()
                                .map(|label| format!("{} ({})", label.name, label.label_type))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                    })
                    .collect()
            }
            RunnerGroups => {
                self.controls.extend(
                    [
                        ActionsCommand::CreateRunnerGroup,
                        ActionsCommand::EditRunnerGroup,
                        ActionsCommand::GroupRunners,
                        ActionsCommand::GroupRepositories,
                        ActionsCommand::DeleteRunnerGroup,
                    ]
                    .into_iter()
                    .map(command),
                );
                self.runner_groups
                    .iter()
                    .map(|group| {
                        ActionsRow::text(format!(
                            "#{} • {} • visibility {} • default {} • inherited {}",
                            group.id, group.name, group.visibility, group.default, group.inherited
                        ))
                    })
                    .collect()
            }
            Usage => self.rows.clone(),
            Reconciliation => {
                self.controls.push(add(
                    "Acknowledge inspected state",
                    ActionsAction::Acknowledge,
                ));
                self.observation
                    .as_ref()
                    .map(|observation| {
                        serde_json::to_string_pretty(observation.state())
                            .unwrap_or_default()
                            .lines()
                            .map(ActionsRow::text)
                            .collect()
                    })
                    .unwrap_or_default()
            }
            Publications | PublicationInspection => self.prepare_publication_rows(),
        };
        if self.rows.is_empty() {
            self.rows.push(ActionsRow::text(if self.view == Artifacts {
                "This run has no artifacts (or they were already deleted)."
            } else {
                "No records reported in this scope."
            }));
        }
        if let Some(id) = selected_environment {
            if let Some(index) = self
                .environment_rows
                .iter()
                .position(|identity| *identity == Some(id))
            {
                self.selected = index;
            }
        }
        if self.view == Logs {
            if let Some((query, regex)) = &self.log_search {
                let matcher = if *regex {
                    regex::RegexBuilder::new(query)
                        .size_limit(1024 * 1024)
                        .dfa_size_limit(1024 * 1024)
                        .build()
                        .ok()
                } else {
                    None
                };
                for row in &mut self.rows {
                    if matcher
                        .as_ref()
                        .map_or(!regex && row.label.contains(query), |matcher| {
                            matcher.is_match(&row.label)
                        })
                    {
                        row.role = SemanticToken::Changed;
                    }
                }
            }
        }
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        self.top = self.top.min(self.rows.len().saturating_sub(1));
        for index in 0..self.controls.len() {
            if let ActionsAction::Compose(command) = self.controls[index].action {
                self.controls[index].disabled_reason = self.disabled_reason(command);
            }
        }
    }
    fn catalog_controls() -> Vec<ActionsControl> {
        ActionsView::CATALOGS
            .into_iter()
            .map(|(view, label)| ActionsControl {
                label: label.into(),
                action: ActionsAction::View(view),
                disabled_reason: None,
            })
            .collect()
    }
    pub fn disabled_reason(&self, command: ActionsCommand) -> Option<String> {
        use ActionsCommand::*;
        let remote_mutation = !matches!(
            command,
            FilterRuns
                | FilterCaches
                | ChooseScope
                | WorkflowRevision
                | SearchLogs
                | ExportArtifact
                | ExtractArtifact
        );
        if remote_mutation && self.reconciliation_target.is_some() {
            return Some("Inspect and acknowledge the uncertain write before composing another mutation; refresh alone does not unlock it".into());
        }
        if remote_mutation && self.identity.account_id.is_none_or(|id| id == 0) {
            return Some("Authenticate a verified account before composing changes".into());
        }
        if let Some(reason) = self
            .publication_leases
            .as_ref()
            .and_then(|snapshot| snapshot.blocking_reason(self, command))
        {
            return Some(reason);
        }
        let run = self.run.as_ref().map(|page| &page.attempt.run);
        let missing = match command {
            RerunAll | RerunFailed | RerunJob | Cancel | ForceCancel | DeleteRun | DeleteLogs
            | ApproveFork | ApproveDeployment | RejectDeployment => run.is_none(),
            EnableWorkflow | DisableWorkflow | DispatchWorkflow => self.definition.is_none(),
            ExportArtifact | ExtractArtifact | DeleteArtifact => self.artifact.is_none(),
            DeleteCache => self.caches.get(self.selected).is_none(),
            DeleteSecret => self.secrets.is_empty(),
            DeleteVariable => self.variables.is_empty(),
            EditPolicy => self.policy.is_none(),
            EditEnvironment => self.selected_environment().is_none(),
            RunnerLabels | RemoveRunner => self.runners.is_empty(),
            EditRunnerGroup | GroupRunners | GroupRepositories | DeleteRunnerGroup => {
                self.runner_groups.is_empty()
            }
            _ => false,
        };
        if missing {
            return Some("Select and inspect this exact target first".into());
        }
        if matches!(command, ExportArtifact | ExtractArtifact) && self.archive.is_none() {
            return Some(match &self.artifact {
                Some(artifact) if artifact_too_large(artifact) => format!(
                    "{} is over the {} in-app limit; use gh run download",
                    human_bytes(artifact.size_in_bytes),
                    human_bytes(ARTIFACT_IN_APP_LIMIT)
                ),
                _ => "Open the artifact to download its archive first".into(),
            });
        }
        if command == EditEnvironment {
            if let Some(environment) = self.selected_environment() {
                if let Err(error) = settings::environment_branch_mode(environment) {
                    return Some(error.message);
                }
            }
        }
        if command == RemoveRunner
            && self
                .runners
                .get(self.selected)
                .is_some_and(|runner| runner.busy)
        {
            return Some("The selected runner is busy; removal is blocked until it is idle. Refresh its exact state before removal.".into());
        }
        if matches!(command, ApproveDeployment | RejectDeployment) {
            let Some(gate) = self.selected_deployment() else {
                return Some(
                    "Select an exact pending environment gate, not an approval-history row".into(),
                );
            };
            if !gate.current_user_can_approve {
                return Some("GitHub reports this account is not eligible: required-reviewer, self-approval, wait-timer or other protection restrictions apply".into());
            }
            if self.accepted_deployments.contains(&gate.environment_id) {
                return Some("GitHub accepted a decision for this environment but still reports it pending. Refresh to observe its transition; do not repeat the accepted decision".into());
            }
        }
        if command == ApproveFork
            && run.is_some_and(|run| {
                run.event != "pull_request"
                    || run.status != "action_required" && run.conclusion != "action_required"
            })
        {
            return Some(
                "This pull-request run is not awaiting fork approval; no approval is sent".into(),
            );
        }
        if matches!(command, RerunAll | RerunFailed | RerunJob)
            && run.is_some_and(|run| run.status != "completed")
        {
            return Some("Only completed runs can be rerun".into());
        }
        if matches!(command, Cancel | ForceCancel)
            && run.is_some_and(|run| run.status == "completed")
        {
            return Some("Run already completed".into());
        }
        if command == RerunJob
            && self
                .run
                .as_ref()
                .is_none_or(|page| page.selected_job.is_none())
        {
            return Some("Open the job to rerun before composing".into());
        }
        if command == DispatchWorkflow
            && self.definition.as_ref().is_none_or(|definition| {
                !definition
                    .triggers
                    .iter()
                    .any(|trigger| trigger == "workflow_dispatch")
            })
        {
            return Some("This definition does not expose workflow_dispatch".into());
        }
        if command == EnableWorkflow
            && self
                .definition
                .as_ref()
                .is_some_and(|definition| definition.workflow.state == "active")
        {
            return Some("Workflow already enabled".into());
        }
        if matches!(command, DisableWorkflow | DispatchWorkflow)
            && self
                .definition
                .as_ref()
                .is_some_and(|definition| definition.workflow.state != "active")
        {
            return Some("Enable this workflow before dispatching or disabling it".into());
        }
        None
    }
}
