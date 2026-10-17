//! Closed catalogue-origin Actions observations. Display/DTOs never construct targets.
use super::attempt_jobs_v1::ExactAttemptJobs;
use super::*;
use std::{collections::BTreeSet, sync::Arc};

const MAX_RUNS: usize = 2000;
const MAX_JOBS: usize = 1000;
const MAX_ANNOTATIONS: usize = 1000;

/// Actual provider-produced run target; not authentication or write permission.
pub struct ScopedActionsRunTargetV1 {
    producer: CommandRunner,
    repository_id: u64,
    repository_node: Option<String>,
    repository: String,
    run: WorkflowRun,
    node: Option<String>,
    suite: Option<u64>,
}
impl std::fmt::Debug for ScopedActionsRunTargetV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedActionsRunTargetV1(<actual observed read target>)")
    }
}
impl ScopedActionsRunTargetV1 {
    pub(crate) fn dispatch_receipt_v7(
        &self,
    ) -> GithubResult<crate::actions_write_journal::ActionsDispatchRunReceiptV7> {
        let node = self
            .node
            .as_ref()
            .filter(|node| bounded_node_v3(node))
            .ok_or_else(|| {
                error(
                    GithubErrorKind::AmbiguousWrite,
                    "Actual Dispatch run node missing",
                )
            })?;
        let repository_node = self
            .repository_node
            .as_ref()
            .filter(|node| bounded_node_v3(node))
            .ok_or_else(|| {
                error(
                    GithubErrorKind::AmbiguousWrite,
                    "Actual Dispatch repository node missing",
                )
            })?;
        let suite = self.suite.filter(|id| *id > 0).ok_or_else(|| {
            error(
                GithubErrorKind::AmbiguousWrite,
                "Actual Dispatch suite missing",
            )
        })?;
        if self.run.event != "workflow_dispatch"
            || self.run.id == 0
            || self.run.attempt == 0
            || validate_sha(&self.run.head_sha).is_err()
        {
            return Err(error(
                GithubErrorKind::AmbiguousWrite,
                "Actual Dispatch receipt identities missing",
            ));
        }
        Ok(crate::actions_write_journal::ActionsDispatchRunReceiptV7 {
            repository_id: self.repository_id,
            repository_node: repository_node.clone(),
            run_id: self.run.id,
            run_node: node.clone(),
            workflow_id: self.run.workflow_id,
            attempt: self.run.attempt,
            head_sha: self.run.head_sha.clone(),
            event: self.run.event.clone(),
            check_suite_id: suite,
        })
    }
    /// Only the SAME producing provider's prepaid HTTP200-ID receipt can call
    /// this factory. Reuse the canonical run decoder, retaining raw identities
    /// that legacy DTOs intentionally do not carry. Never a saved-row issuer.
    pub(in crate::providers::gh) fn from_dispatch_raw_v7(
        provider: &GithubProvider,
        schema: &Arc<ScopedDispatchSchemaV7>,
        id: u64,
        raw: &Value,
    ) -> GithubResult<Arc<Self>> {
        let run = run_from_json(raw);
        let node = bounded_optional_node(raw)?;
        let suite = raw
            .get("check_suite_id")
            .and_then(Value::as_u64)
            .filter(|id| *id > 0 && *id <= i64::MAX as u64);
        if !schema.producing_provider_v7(provider)
            || id == 0
            || id > i64::MAX as u64
            || raw["id"].as_u64() != Some(id)
            || run.id != id
            || run.workflow_id != schema.original().model().id
            || run.head_sha != schema.commit()
            || run.event != "workflow_dispatch"
            || raw["run_attempt"]
                .as_u64()
                .filter(|a| *a > 0 && *a <= i64::MAX as u64)
                != Some(run.attempt)
            || node.is_none()
            || suite.is_none()
            || raw["repository"]["id"].as_u64() != Some(schema.original().repository().id())
            || raw["repository"]["node_id"].as_str() != schema.original().repository().node()
            || raw["repository"]["full_name"].as_str() != Some(schema.repository())
            || validate_sha(&run.head_sha).is_err()
        {
            return Err(error(GithubErrorKind::AmbiguousWrite,"Actual Dispatch run/repository/node/suite/event/head differs; no run target adopted"));
        }
        Ok(Arc::new(Self {
            producer: provider.runner.clone(),
            repository_id: schema.original().repository().id(),
            repository_node: schema.original().repository().node().map(str::to_owned),
            repository: schema.repository().into(),
            run,
            node,
            suite,
        }))
    }
    pub(crate) fn node_v2(&self) -> Option<&str> {
        self.node.as_deref()
    }
    pub(crate) fn suite_v3(&self) -> Option<u64> {
        self.suite
    }
    pub(crate) fn original_writer_v3(&self, provider: &GithubProvider) -> bool {
        self.producer.same_instance(&provider.runner)
            && self.repository_id > 0
            && self.repository_id <= i64::MAX as u64
            && self.repository_node.as_deref().is_some_and(bounded_node_v3)
            && self.node.as_deref().is_some_and(bounded_node_v3)
            && self.suite.is_none_or(|id| id > 0 && id <= i64::MAX as u64)
            && self.run.id > 0
            && self.run.id <= i64::MAX as u64
            && self.run.workflow_id > 0
            && self.run.attempt > 0
            && validate_sha(&self.run.head_sha).is_ok()
    }
    pub(crate) fn same_watch_pin_v2(&self, old: &Self) -> bool {
        self.producer.same_instance(&old.producer)
            && self.repository_id == old.repository_id
            && self.repository_node == old.repository_node
            && self.node == old.node
            && self.suite == old.suite
            && run_identity(&self.run, &old.run)
    }
    pub(crate) fn from_pr_observation_v2(
        observed: &Arc<PrCheckRunObservation>,
    ) -> GithubResult<Arc<Self>> {
        let selected = observed.selected_run_v1().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Genuine selected PR run unavailable",
            )
        })?;
        let run = &selected.attempt().run;
        if run.id == 0
            || run.attempt == 0
            || run.head_sha != observed.head_sha()
            || selected.node_v2().is_empty()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual PR run/head/node required",
            ));
        }
        Ok(Arc::new(Self {
            producer: observed.producing_runner_v2().clone(),
            repository_id: observed.repository().id,
            repository_node: Some(observed.repository().node_id.clone()),
            repository: observed.repository().full_name.clone(),
            run: run.clone(),
            node: Some(selected.node_v2().into()),
            suite: Some(observed.suite_id()),
        }))
    }
    pub(super) fn current_resource_v2(
        &self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
    ) -> bool {
        self.current(provider, current)
    }
    pub fn run(&self) -> &WorkflowRun {
        &self.run
    }
    pub fn repository(&self) -> &str {
        &self.repository
    }
    pub fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub fn repository_node(&self) -> Option<&str> {
        self.repository_node.as_deref()
    }
    pub(crate) fn owned_bytes_v1(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.repository.capacity()
            + self.repository_node.as_ref().map_or(0, String::capacity)
            + self.node.as_ref().map_or(0, String::capacity)
            + crate::actions_workspace::scoped_read_v1::run_owned_bytes(&self.run)
    }
    fn current(&self, provider: &GithubProvider, row: &CurrentCatalogueRepository) -> bool {
        provider.runner.same_instance(&self.producer)
            && row.repository().id == self.repository_id
            && row.repository().node_id == self.repository_node
            && row.repository().permissions.pull == Some(true)
    }
}
pub struct ScopedActionsJobTargetV1 {
    run: Arc<ScopedActionsRunTargetV1>,
    id: u64,
    node: Option<String>,
    check_url: String,
}
impl std::fmt::Debug for ScopedActionsJobTargetV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedActionsJobTargetV1(<actual exact-attempt member>)")
    }
}
impl ScopedActionsJobTargetV1 {
    pub(super) fn check_url_v4(&self) -> &str {
        &self.check_url
    }
    pub(crate) fn node_v2(&self) -> Option<&str> {
        self.node.as_deref()
    }
    pub(crate) fn from_pr_observation_v2(
        observed: &Arc<PrCheckRunObservation>,
        run: &Arc<ScopedActionsRunTargetV1>,
    ) -> GithubResult<Arc<[Arc<Self>]>> {
        let selected = observed.selected_run_v1().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Original selected PR jobs unavailable",
            )
        })?;
        let mut result = Vec::with_capacity(selected.members().len());
        for member in selected.members() {
            let job = selected
                .attempt()
                .jobs
                .iter()
                .find(|job| job.id == member.id())
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::IdentityChanged,
                        "Genuine member/model differs",
                    )
                })?;
            if job.run_id != run.run.id
                || job.head_sha != run.run.head_sha
                || job.check_run_url.len() > 4096
            {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Original PR job differs",
                ));
            }
            result.push(Arc::new(Self {
                run: run.clone(),
                id: member.id(),
                node: Some(member.node().into()),
                check_url: job.check_run_url.clone(),
            }));
        }
        Ok(result.into())
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn run(&self) -> &Arc<ScopedActionsRunTargetV1> {
        &self.run
    }
    pub(crate) fn owned_bytes_v1(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.node.as_ref().map_or(0, String::capacity)
            + self.check_url.capacity()
    }
}
pub struct ScopedActionsCatalogueReadV1 {
    producer: CommandRunner,
    repository_id: u64,
    repository_node: Option<String>,
    repository: String,
    targets: Arc<[Arc<ScopedActionsRunTargetV1>]>,
    catalog: RunCatalog,
    window: Arc<ScopedRunWindowV2>,
}
impl ScopedActionsCatalogueReadV1 {
    pub(crate) fn window(&self) -> &Arc<ScopedRunWindowV2> {
        &self.window
    }
    pub(crate) fn catalogue(&self) -> &RunCatalog {
        &self.catalog
    }
    pub fn targets(&self) -> &Arc<[Arc<ScopedActionsRunTargetV1>]> {
        &self.targets
    }
    pub fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub fn repository_node(&self) -> Option<&str> {
        self.repository_node.as_deref()
    }
    pub fn repository(&self) -> &str {
        &self.repository
    }
    pub fn same_producer(&self, provider: &GithubProvider) -> bool {
        self.producer.same_instance(&provider.runner)
    }
    pub fn into_parts(
        self,
    ) -> (
        RunCatalog,
        Arc<[Arc<ScopedActionsRunTargetV1>]>,
        Arc<ScopedRunWindowV2>,
    ) {
        (self.catalog, self.targets, self.window)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedActionsReadKindV1 {
    Attempt,
    Logs,
    Annotations,
}

pub enum ScopedActionsReadBodyV1 {
    Attempt,
    Logs(Arc<ScopedActionsLogSnapshotV2>),
    Annotations(Vec<CheckAnnotation>),
    Unavailable(&'static str),
}
pub struct ScopedActionsAttemptReadV1 {
    requested: Arc<ScopedActionsRunTargetV1>,
    selected: Option<Arc<ScopedActionsJobTargetV1>>,
    observed: Arc<ScopedActionsRunTargetV1>,
    jobs: Arc<[Arc<ScopedActionsJobTargetV1>]>,
    attempt: RunAttempt,
    body: ScopedActionsReadBodyV1,
}
impl ScopedActionsAttemptReadV1 {
    pub fn requested(&self) -> &Arc<ScopedActionsRunTargetV1> {
        &self.requested
    }
    pub fn selected(&self) -> Option<&Arc<ScopedActionsJobTargetV1>> {
        self.selected.as_ref()
    }
    pub fn target(&self) -> &Arc<ScopedActionsRunTargetV1> {
        &self.observed
    }
    pub fn jobs(&self) -> &Arc<[Arc<ScopedActionsJobTargetV1>]> {
        &self.jobs
    }
    pub fn into_parts(self) -> (RunAttempt, ScopedActionsReadBodyV1) {
        (self.attempt, self.body)
    }
}
fn bounded_optional_node(raw: &Value) -> GithubResult<Option<String>> {
    match raw.get("node_id") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s))
            if !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control) =>
        {
            Ok(Some(s.clone()))
        }
        _ => Err(error(
            GithubErrorKind::Json,
            "Observed Actions node exceeds its bounded identity field",
        )),
    }
}
fn run_identity(actual: &WorkflowRun, wanted: &WorkflowRun) -> bool {
    actual.id == wanted.id
        && actual.workflow_id == wanted.workflow_id
        && actual.attempt == wanted.attempt
        && actual.head_sha == wanted.head_sha
        && actual.event == wanted.event
}
fn bounded_node_v3(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
#[derive(Clone, Copy)]
pub(super) enum ScopedActionsRunRawPhaseV3 {
    Read,
    UnchangedPrewrite,
    CancelReceipt,
    RerunReceipt,
    HistoricalReadV8,
}
/// ONE binder for the existing actual read and the exact prepaid receipt GET.
/// A Value or DTO is never promoted into a new original target by this helper.
pub(super) fn bind_scoped_actions_run_raw_v3(
    raw: &Value,
    repo: &str,
    target: &ScopedActionsRunTargetV1,
    phase: ScopedActionsRunRawPhaseV3,
) -> GithubResult<(WorkflowRun, Option<String>, Option<u64>)> {
    let run = run_from_json(raw);
    let node = bounded_optional_node(raw)?;
    let suite = raw
        .get("check_suite_id")
        .and_then(Value::as_u64)
        .filter(|id| *id > 0);
    let same_attempt = run.attempt == target.run.attempt
        || matches!(
            phase,
            ScopedActionsRunRawPhaseV3::RerunReceipt | ScopedActionsRunRawPhaseV3::HistoricalReadV8
        ) && run.attempt >= target.run.attempt;
    if !same_attempt
        || run.id != target.run.id
        || run.workflow_id != target.run.workflow_id
        || run.head_sha != target.run.head_sha
        || run.event != target.run.event
        || raw["repository"]["id"].as_u64() != Some(target.repository_id)
        || target
            .repository_node
            .as_deref()
            .is_some_and(|known| raw["repository"]["node_id"].as_str() != Some(known))
        || raw["repository"]["full_name"].as_str() != Some(repo)
        || target
            .node
            .as_ref()
            .is_some_and(|known| node.as_ref() != Some(known))
        || target.suite.is_some_and(|known| suite != Some(known))
    {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Original run/workflow/attempt/head/repository/node/suite changed; no replacement adopted",
        ));
    }
    if !matches!(phase, ScopedActionsRunRawPhaseV3::Read)
        && (target
            .repository_node
            .as_deref()
            .is_none_or(|s| !bounded_node_v3(s))
            || target.node.as_deref().is_none_or(|s| !bounded_node_v3(s))
            || suite != target.suite
            || raw.get("check_suite_id").is_some_and(|value| {
                !value.is_null()
                    && value
                        .as_u64()
                        .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            })
            || run.id == 0
            || run.attempt == 0
            || raw
                .get("run_attempt")
                .and_then(Value::as_u64)
                .filter(|a| *a > 0)
                != Some(run.attempt)
            || run.workflow_id == 0
            || validate_sha(&run.head_sha).is_err())
    {
        return Err(error(
            GithubErrorKind::Unsupported,
            "Original scoped run publication requires actual repository and run nodes",
        ));
    }
    if matches!(phase, ScopedActionsRunRawPhaseV3::UnchangedPrewrite)
        && (run.status != target.run.status || run.conclusion != target.run.conclusion)
    {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Run state changed after Preview; inspect the actual current run before confirming",
        ));
    }
    Ok((run, node, suite))
}
fn check_cancel(cancel: &CancellationToken) -> GithubResult<()> {
    if cancel.is_cancelled() {
        Err(error(
            GithubErrorKind::Transport,
            "Original Actions read cancelled",
        ))
    } else {
        Ok(())
    }
}
fn unavailable(error: &GithubError) -> bool {
    matches!(
        error.kind,
        GithubErrorKind::Permission | GithubErrorKind::NotFound
    )
}
impl GithubProvider {
    /// One genuine bounded run GET under the existing original Session. Only
    /// the caller's same original source/SQL/final-principal close may publish
    /// this candidate; it grants no access/confirmation/write permission.
    pub(crate) fn scoped_actions_run_snapshot_v3(
        &self,
        current: &CurrentCatalogueRepository,
        original: &Arc<ScopedActionsRunTargetV1>,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<ScopedActionsRunTargetV1>> {
        if !original.current(self, current) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Actual original numeric run source changed",
            ));
        }
        let (run, node, suite) =
            self.scoped_actions_run_raw_v1(&current.repository().full_name, original, cancel)?;
        let value = Arc::new(ScopedActionsRunTargetV1 {
            producer: self.runner.clone(),
            repository_id: current.repository().id,
            repository_node: current.repository().node_id.clone(),
            repository: current.repository().full_name.clone(),
            run,
            node,
            suite,
        });
        check_cancel(cancel)?;
        Ok(value)
    }
    pub(super) fn scoped_resource_run_current_v2(
        &self,
        current: &CurrentCatalogueRepository,
        target: &ScopedActionsRunTargetV1,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if !target.current(self, current) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual original run source required",
            ));
        }
        self.scoped_actions_run_raw_v1(&current.repository().full_name, target, cancel)
            .map(|_| ())
    }
    /// Same existing catalogue transport/parser; only this real read mints targets.
    pub fn scoped_actions_catalogue_v1(
        &self,
        current: &CurrentCatalogueRepository,
        filter: &RunFilter,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedActionsCatalogueReadV1> {
        self.scoped_actions_catalogue_page_v2(
            current,
            ScopedRunQueryV2::first(filter.clone())?,
            cancel,
        )
    }
    pub fn scoped_actions_catalogue_page_v2(
        &self,
        current: &CurrentCatalogueRepository,
        query: Arc<ScopedRunQueryV2>,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedActionsCatalogueReadV1> {
        let row = current.repository();
        if row.id == 0 || row.permissions.pull != Some(true) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Current original Actions repository required",
            ));
        }
        let (catalog, window) = self
            .scoped_run_page_v2(&row.full_name, query, cancel)?
            .into_parts();
        if catalog.runs.len() > MAX_RUNS {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Actions catalogue exceeds bounded observed page coverage",
            ));
        }
        let proof_bound = catalog.runs.iter().fold(0usize, |sum, run| {
            sum.saturating_add(
                crate::actions_workspace::scoped_read_v1::run_owned_bytes(run)
                    .saturating_mul(4)
                    .saturating_add(2048),
            )
        });
        if proof_bound > crate::scoped_actions_v1::PREPARED_LIMIT_V1 {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Catalogue proof/presentation preflight exceeds8MiB; no target clone made",
            ));
        }
        let targets: Arc<[_]> = catalog
            .runs
            .iter()
            .map(|run| {
                Arc::new(ScopedActionsRunTargetV1 {
                    producer: self.runner.clone(),
                    repository_id: row.id,
                    repository_node: row.node_id.clone(),
                    repository: row.full_name.clone(),
                    run: run.clone(),
                    node: None,
                    suite: None,
                })
            })
            .collect::<Vec<_>>()
            .into();
        check_cancel(cancel)?;
        Ok(ScopedActionsCatalogueReadV1 {
            producer: self.runner.clone(),
            repository_id: row.id,
            repository_node: row.node_id.clone(),
            repository: row.full_name.clone(),
            targets,
            catalog,
            window,
        })
    }
    pub(super) fn scoped_actions_run_raw_v1(
        &self,
        repo: &str,
        target: &ScopedActionsRunTargetV1,
        cancel: &CancellationToken,
    ) -> GithubResult<(WorkflowRun, Option<String>, Option<u64>)> {
        self.scoped_actions_run_raw_phase_v3(repo, target, ScopedActionsRunRawPhaseV3::Read, cancel)
    }
    pub(super) fn scoped_actions_run_raw_phase_v3(
        &self,
        repo: &str,
        target: &ScopedActionsRunTargetV1,
        phase: ScopedActionsRunRawPhaseV3,
        cancel: &CancellationToken,
    ) -> GithubResult<(WorkflowRun, Option<String>, Option<u64>)> {
        let raw = self.api(
            &format!("repos/{repo}/actions/runs/{}", target.run.id),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        bind_scoped_actions_run_raw_v3(&raw, repo, target, phase)
    }
    fn scoped_actions_job_raw_v1(
        &self,
        repo: &str,
        endpoint: &ExactAttemptJobs,
        target: &ScopedActionsJobTargetV1,
        cancel: &CancellationToken,
    ) -> GithubResult<ActionsJob> {
        let raw = self.api(
            &format!("repos/{repo}/actions/jobs/{}", target.id),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let (job, _) = endpoint.bind_job(&raw)?;
        let node = bounded_optional_node(&raw)?;
        if job.id != target.id
            || target
                .node
                .as_ref()
                .is_some_and(|old| node.as_ref() != Some(old))
            || job.check_run_url != target.check_url
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original selected job or check link changed",
            ));
        }
        Ok(job)
    }
    fn scoped_actions_check_v1(
        &self,
        repo: &str,
        target: &ScopedActionsRunTargetV1,
        job: &ActionsJob,
        cancel: &CancellationToken,
    ) -> GithubResult<(u64, String, u64)> {
        let prefix = format!(
            "https://{}/repos/{repo}/check-runs/",
            if self.host == "github.com" {
                "api.github.com".into()
            } else {
                format!("{}/api/v3", self.host)
            }
        );
        let id = job
            .check_run_url
            .strip_prefix(&prefix)
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|id| *id > 0)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Unsupported,
                    "Job has no canonical check selector; no annotation source guessed",
                )
            })?;
        let raw = self.api(
            &format!("repos/{repo}/check-runs/{id}"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let node = bounded_optional_node(&raw)?.ok_or_else(|| {
            error(
                GithubErrorKind::Unsupported,
                "Actual CheckRun node unavailable",
            )
        })?;
        let suite = raw["check_suite"]["id"]
            .as_u64()
            .filter(|id| *id > 0)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Unsupported,
                    "Actual CheckRun suite unavailable",
                )
            })?;
        if raw["id"].as_u64() != Some(id)
            || raw["head_sha"].as_str() != Some(target.run.head_sha.as_str())
            || raw["url"].as_str() != Some(job.check_run_url.as_str())
            || target.suite.is_some_and(|known| known != suite)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Actual job CheckRun ID/head/suite/source changed",
            ));
        }
        Ok((id, node, suite))
    }
    /// One original measured window/token/absolute deadline from the caller.
    /// This closed read never accepts a public DTO as a source or writes.
    pub fn scoped_actions_attempt_v1(
        &self,
        current: &CurrentCatalogueRepository,
        requested: Arc<ScopedActionsRunTargetV1>,
        selected: Option<Arc<ScopedActionsJobTargetV1>>,
        kind: ScopedActionsReadKindV1,
        cancel: &CancellationToken,
        mut guard: impl FnMut() -> GithubResult<()>,
    ) -> GithubResult<ScopedActionsAttemptReadV1> {
        if !requested.current(self, current)
            || selected
                .as_ref()
                .is_some_and(|job| !Arc::ptr_eq(&job.run, &requested))
            || kind != ScopedActionsReadKindV1::Attempt && selected.is_none()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual original Actions source/member required",
            ));
        }
        let repo = &current.repository().full_name;
        let (run, node, suite) = self.scoped_actions_run_raw_v1(repo, &requested, cancel)?;
        let endpoint = ExactAttemptJobs::read(self, repo, run.id, run.attempt, cancel)?;
        if !run_identity(&endpoint.run, &run)
            || endpoint.repository_id != Some(requested.repository_id)
            || requested
                .repository_node
                .as_ref()
                .is_some_and(|node| endpoint.repository_node.as_ref() != Some(node))
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Exact attempt differs from original run",
            ));
        }
        let raw_jobs = self.api_list(&endpoint.jobs_endpoint, Some("jobs"), cancel)?;
        if raw_jobs.len() > MAX_JOBS {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Observed job catalogue exceeds 1000 member bound",
            ));
        }
        let observed = Arc::new(ScopedActionsRunTargetV1 {
            producer: self.runner.clone(),
            repository_id: requested.repository_id,
            repository_node: requested.repository_node.clone(),
            repository: repo.clone(),
            run,
            node,
            suite,
        });
        let mut ids = BTreeSet::new();
        let mut jobs = Vec::with_capacity(raw_jobs.len());
        let mut targets = Vec::with_capacity(raw_jobs.len());
        for raw in &raw_jobs {
            let (job, _) = endpoint.bind_job(raw)?;
            if !ids.insert(job.id) || job.check_run_url.len() > 4096 {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Duplicate/oversized actual Actions member",
                ));
            }
            targets.push(Arc::new(ScopedActionsJobTargetV1 {
                run: observed.clone(),
                id: job.id,
                node: bounded_optional_node(raw)?,
                check_url: job.check_run_url.clone(),
            }));
            jobs.push(job);
        }
        drop(raw_jobs);
        let current_job = if let Some(wanted) = &selected {
            let actual = targets
                .iter()
                .find(|job| job.id == wanted.id)
                .filter(|job| {
                    wanted
                        .node
                        .as_ref()
                        .is_none_or(|node| job.node.as_ref() == Some(node))
                        && job.check_url == wanted.check_url
                })
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::StaleHead,
                        "Original selected member unavailable; no broader logs used",
                    )
                })?;
            Some((
                self.scoped_actions_job_raw_v1(repo, &endpoint, actual, cancel)?,
                actual.clone(),
            ))
        } else {
            None
        };
        if kind != ScopedActionsReadKindV1::Annotations {
            guard()?;
            check_cancel(cancel)?;
        }
        let body = match kind {
            ScopedActionsReadKindV1::Attempt => ScopedActionsReadBodyV1::Attempt,
            ScopedActionsReadKindV1::Logs => {
                let (job, target) = current_job.as_ref().expect("selected member checked");
                match self.scoped_actions_capture_logs_v2(
                    current,
                    observed.clone(),
                    target.clone(),
                    job,
                    cancel,
                ) {
                    Ok(snapshot) => ScopedActionsReadBodyV1::Logs(snapshot),
                    Err(e) if unavailable(&e) => ScopedActionsReadBodyV1::Unavailable(
                        "Selected job logs unavailable on this host/account",
                    ),
                    Err(e) => return Err(e),
                }
            }
            ScopedActionsReadKindV1::Annotations => {
                let job = &current_job.as_ref().expect("selected member checked").0;
                match self.scoped_actions_check_v1(repo, &observed, job, cancel) {
                    Ok(check) => {
                        guard()?;
                        check_cancel(cancel)?;
                        let rows = self.job_annotations(repo, job, cancel);
                        let closing = self.scoped_actions_check_v1(repo, &observed, job, cancel)?;
                        if check != closing {
                            return Err(error(
                                GithubErrorKind::IdentityChanged,
                                "Actual annotation CheckRun changed during read",
                            ));
                        }
                        match rows {
                            Ok(rows) if rows.len() <= MAX_ANNOTATIONS => {
                                ScopedActionsReadBodyV1::Annotations(rows)
                            }
                            Ok(_) => {
                                return Err(error(
                                    GithubErrorKind::Unsupported,
                                    "Annotation view exceeds bounded reported rows",
                                ))
                            }
                            Err(e) if unavailable(&e) => ScopedActionsReadBodyV1::Unavailable(
                                "Selected job annotations unavailable on this host/account",
                            ),
                            Err(e) => return Err(e),
                        }
                    }
                    Err(e) if unavailable(&e) || matches!(e.kind, GithubErrorKind::Unsupported) => {
                        ScopedActionsReadBodyV1::Unavailable(
                            "Actual canonical annotation source unavailable; nothing guessed",
                        )
                    }
                    Err(e) => return Err(e),
                }
            }
        };
        if let Some((_, target)) = &current_job {
            self.scoped_actions_job_raw_v1(repo, &endpoint, target, cancel)?;
        }
        let (_, closing_node, closing_suite) =
            self.scoped_actions_run_raw_v1(repo, &observed, cancel)?;
        if closing_node != observed.node || closing_suite != observed.suite {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original run node/suite changed during read",
            ));
        }
        let attempt = RunAttempt { run: observed.run.clone(), jobs,
            definition_note: "Actual exact-attempt jobs • omitted optional attempts bound only by this read. Matrix/reusable dependency edges unavailable; expanded names/labels are factual, not inferred edges.".into() };
        let result = ScopedActionsAttemptReadV1 {
            requested,
            selected,
            observed,
            jobs: targets.into(),
            attempt,
            body,
        };
        check_cancel(cancel)?;
        Ok(result)
    }
}

#[cfg(test)]
#[path = "scoped_read_v1_tests.rs"]
mod tests;
