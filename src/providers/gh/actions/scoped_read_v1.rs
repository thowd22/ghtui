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
    pub(crate) fn node_v2(&self) -> Option<&str> {
        self.node.as_deref()
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
    fn scoped_actions_run_raw_v1(
        &self,
        repo: &str,
        target: &ScopedActionsRunTargetV1,
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
        let run = run_from_json(&raw);
        let node = bounded_optional_node(&raw)?;
        let suite = raw
            .get("check_suite_id")
            .and_then(Value::as_u64)
            .filter(|id| *id > 0);
        if !run_identity(&run, &target.run)
            || raw["repository"]["id"].as_u64() != Some(target.repository_id)
            || target
                .repository_node
                .as_deref()
                .is_some_and(|node| raw["repository"]["node_id"].as_str() != Some(node))
            || raw["repository"]["full_name"].as_str() != Some(repo)
            || target
                .node
                .as_ref()
                .is_some_and(|known| node.as_ref() != Some(known))
            || target.suite.is_some_and(|known| suite != Some(known))
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Original run/workflow/attempt/head changed; no latest attempt adopted",
            ));
        }
        Ok((run, node, suite))
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
