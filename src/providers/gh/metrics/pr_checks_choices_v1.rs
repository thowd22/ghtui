//! SOURCE-ONLY child of the existing bridge. Opaque observed choices, shared
//! exact-attempt kernel and selected-read policy. No new client/auth/publisher.
use super::*;

pub struct PrCheckRunCandidateV1 {
    run: WorkflowRun,
    node: String,
}
impl PrCheckRunCandidateV1 {
    pub fn run(&self) -> &WorkflowRun {
        &self.run
    }
    pub fn node(&self) -> &str {
        &self.node
    }
}
pub struct PrCheckJobMemberV1 {
    id: u64,
    node: String,
    source: PrCheckJobAttemptOrigin,
    original_check: bool,
}
impl PrCheckJobMemberV1 {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn node(&self) -> &str {
        &self.node
    }
    pub fn source(&self) -> PrCheckJobAttemptOrigin {
        self.source
    }
    pub fn maps_original_check(&self) -> bool {
        self.original_check
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCheckSelectionOriginV1 {
    ObservedOnly,
    ExplicitRun,
    ExplicitJob,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCheckChoiceUnavailableV1 {
    SelectedMemberNotObserved,
    PartialMemberCatalogue,
}
impl PrCheckChoiceUnavailableV1 {
    pub fn label(self) -> &'static str {
        match self {
        Self::SelectedMemberNotObserved=>"Chosen job not present in the exhausted API-visible attempt list; no historical deletion inferred",
        Self::PartialMemberCatalogue=>"Chosen job not found in the bounded partial attempt list; unseen membership is unknown",
    }
    }
}
pub struct PrCheckSelectedRunV1 {
    pub(super) attempt: Arc<RunAttempt>,
    pub(super) run_node: String,
    pub(super) members: Vec<PrCheckJobMemberV1>,
    pub(super) selected_job: Option<u64>,
    pub(super) selection: PrCheckSelectionOriginV1,
}
impl PrCheckSelectedRunV1 {
    pub fn attempt(&self) -> &Arc<RunAttempt> {
        &self.attempt
    }
    pub fn members(&self) -> &[PrCheckJobMemberV1] {
        &self.members
    }
    pub fn selected_job(&self) -> Option<u64> {
        self.selected_job
    }
    pub fn selection(&self) -> PrCheckSelectionOriginV1 {
        self.selection
    }
}
pub struct PrCheckRunChoiceV1 {
    origin: Arc<PrCheckRunObservation>,
    index: usize,
}
pub struct PrCheckJobChoiceV1 {
    origin: Arc<PrCheckRunObservation>,
    index: usize,
}
pub enum PrCheckChoiceV1 {
    Run(PrCheckRunChoiceV1),
    Job(PrCheckJobChoiceV1),
}
impl fmt::Debug for PrCheckChoiceV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrCheckChoiceV1(<opaque observed navigation; not access>)")
    }
}
impl PrCheckChoiceV1 {
    pub fn belongs_to(&self, origin: &Arc<PrCheckRunObservation>) -> bool {
        Arc::ptr_eq(self.origin(), origin)
    }
    fn origin(&self) -> &Arc<PrCheckRunObservation> {
        match self {
            Self::Run(c) => &c.origin,
            Self::Job(c) => &c.origin,
        }
    }
    pub(super) fn run(&self) -> GithubResult<ObservedRun> {
        match self {
            Self::Run(c) => c.origin.run_candidates.get(c.index).map(|v| ObservedRun {
                raw_node: v.node.clone(),
                run: v.run.clone(),
            }),
            Self::Job(c) => c.origin.selected_run.as_ref().map(|v| ObservedRun {
                raw_node: v.run_node.clone(),
                run: v.attempt.run.clone(),
            }),
        }
        .ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Original opaque run choice unavailable",
            )
        })
    }
    pub(super) fn job(&self) -> GithubResult<Option<&PrCheckJobMemberV1>> {
        match self {
            Self::Run(_) => Ok(None),
            Self::Job(c) => c
                .origin
                .selected_run
                .as_ref()
                .and_then(|v| v.members.get(c.index))
                .map(Some)
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::InvalidTarget,
                        "Original opaque job choice unavailable",
                    )
                }),
        }
    }
}
impl PrCheckRunObservation {
    pub fn choice_unavailable_v1(&self) -> Option<PrCheckChoiceUnavailableV1> {
        self.choice_unavailable
    }
    pub fn run_candidates_v1(&self) -> &[PrCheckRunCandidateV1] {
        &self.run_candidates
    }
    pub fn selected_run_v1(&self) -> Option<&Arc<PrCheckSelectedRunV1>> {
        self.selected_run.as_ref()
    }
    pub fn choose_run_v1(self: &Arc<Self>, index: usize) -> GithubResult<PrCheckChoiceV1> {
        if self.run_candidates.get(index).is_none() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "No observed run at that exact row",
            ));
        }
        Ok(PrCheckChoiceV1::Run(PrCheckRunChoiceV1 {
            origin: self.clone(),
            index,
        }))
    }
    pub fn choose_job_v1(self: &Arc<Self>, index: usize) -> GithubResult<PrCheckChoiceV1> {
        if self
            .selected_run
            .as_ref()
            .and_then(|v| v.members.get(index))
            .is_none()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "No genuine observed member at that exact row",
            ));
        }
        Ok(PrCheckChoiceV1::Job(PrCheckJobChoiceV1 {
            origin: self.clone(),
            index,
        }))
    }
}
pub(super) fn retain_candidates_v1(
    runs: &BTreeMap<u64, ObservedRun>,
    limits: &PrCheckRunReadLimits,
) -> GithubResult<Vec<PrCheckRunCandidateV1>> {
    let mut remaining = limits.max_prepared_bytes;
    let mut output = Vec::with_capacity(runs.len().min(limits.max_rows));
    for observed in runs.values() {
        // Individual input rows were already capped BEFORE ObservedRun parsing.
        // Reserve the additional retained DTO footprint before cloning it.
        let bytes = serde_json::to_vec(&observed.run)
            .map_err(|_| error(GithubErrorKind::Json, "Candidate metadata malformed"))?
            .len();
        let bytes=bytes.checked_add(observed.raw_node.len()).and_then(|n|n.checked_add(std::mem::size_of::<PrCheckRunCandidateV1>()))
            .filter(|n|*n<=remaining).ok_or_else(||error(GithubErrorKind::Unsupported,"Retained run candidate display budget reached; no unique/empty candidate inferred"))?;
        if output.len() >= limits.max_rows {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Retained run candidate count reached",
            ));
        }
        remaining -= bytes;
        output.push(PrCheckRunCandidateV1 {
            run: observed.run.clone(),
            node: observed.raw_node.clone(),
        });
    }
    Ok(output)
}
/// SAME actual attempt/jobs/latest-run reader is forwarded by AutoUnique and
/// explicit choices. All network, parsing, paging and projection helpers shared.
pub(super) struct AttemptJobsReadV1 {
    pub attempt: Arc<RunAttempt>,
    pub run_node: String,
    pub sources: BTreeMap<u64, PrCheckJobAttemptOrigin>,
    pub members: Vec<PrCheckJobMemberV1>,
    pub matched: Vec<u64>,
    pub coverage: PrCheckBridgePageCoverage,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn attempt_jobs_v1(
    provider: &GithubProvider,
    original: &PrCheckOriginal,
    repo: &MetricsRepository,
    check: &CheckProof,
    run: &ObservedRun,
    budget: &mut MetricsBudget,
    limits: &PrCheckRunReadLimits,
    shared_rows: &mut usize,
    cancel: &CancellationToken,
    retain: bool,
    after_members: impl FnOnce(&[PrCheckJobMemberV1]) -> GithubResult<()>,
) -> GithubResult<AttemptJobsReadV1> {
    let repo_path = format!("repos/{}", original.repository);
    let check_path = format!("{repo_path}/check-runs/{}", original.check_id);
    let run_path = format!("{repo_path}/actions/runs/{}", run.run.id);
    let attempt_path = format!("{run_path}/attempts/{}", run.run.attempt);
    let attempt = observed_run(
        provider,
        original,
        repo,
        check,
        &budget.read_document(provider, &attempt_path, cancel)?,
    )?;
    same_run(run, &attempt)?;
    let proof = ExactAttemptEndpoint {
        repository: original.repository.clone(),
        run_id: attempt.run.id,
        attempt: attempt.run.attempt,
        head: attempt.run.head_sha.clone(),
    };
    let (raw_jobs, coverage) = pages(
        provider,
        budget,
        limits,
        PageRequest {
            endpoint: &format!("{attempt_path}/jobs"),
            field: "jobs",
            max_pages: limits.max_job_pages,
        },
        shared_rows,
        cancel,
    )?;
    let mut jobs = BTreeMap::new();
    let mut sources = BTreeMap::new();
    let mut members = vec![];
    let mut matched = vec![];
    let mut member_bytes = 0usize;
    let mut projection = JobProjectionBudget::new(limits);
    for raw in &raw_jobs {
        let (job, source) = job_at(raw, &proof, &mut projection)?;
        if !exact_api_url(
            &provider.host,
            &format!("repos/{}/actions/jobs/{}", proof.repository, job.id),
            &raw["url"],
        ) || raw["check_run_url"]
            .as_str()
            .is_none_or(|url| url.len() > 4096)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Job source differs from genuine exact-attempt member",
            ));
        }
        if jobs.contains_key(&job.id) {
            return Err(error(
                GithubErrorKind::Json,
                "Duplicate genuine job ID; no membership alias",
            ));
        }
        let maps = exact_api_url(&provider.host, &check_path, &raw["check_run_url"]);
        if maps {
            matched.push(job.id);
        }
        if retain {
            let node = node(raw, "node_id")?;
            member_bytes = member_bytes
                .checked_add(node.len())
                .and_then(|n| n.checked_add(std::mem::size_of::<PrCheckJobMemberV1>()))
                .filter(|n| *n <= limits.max_prepared_bytes)
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::Unsupported,
                        "Retained genuine job member budget reached",
                    )
                })?;
            members.push(PrCheckJobMemberV1 {
                id: job.id,
                node,
                source,
                original_check: maps,
            });
        }
        sources.insert(job.id, source);
        jobs.insert(job.id, job);
    }
    budget.pr_check_guard(cancel, || after_members(&members))?;
    let closing = observed_run(
        provider,
        original,
        repo,
        check,
        &budget.read_document(provider, &run_path, cancel)?,
    )?;
    same_run(&attempt, &closing)?;
    let source = match matched.as_slice() {
        [id] => match sources.get(id) {
            Some(PrCheckJobAttemptOrigin::JsonObserved) => "Attempt source: supplied job JSON",
            Some(PrCheckJobAttemptOrigin::ExactEndpointBound) => {
                "Attempt source: verified exact endpoint (JSON omitted it)"
            }
            None => "Attempt source: unavailable",
        },
        _ => {
            "Attempt source: genuine per-job provenance; selected CheckRun job mapping unavailable"
        }
    };
    let note=format!("{source}. States were read in a bounded interval, not an atomic snapshot; matrix/reusable dependency edges are unavailable unless independently observed.");
    Ok(AttemptJobsReadV1 {
        attempt: Arc::new(RunAttempt {
            run: closing.run,
            jobs: jobs.into_values().collect(),
            definition_note: note,
        }),
        run_node: closing.raw_node,
        sources,
        members,
        matched,
        coverage,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCheckSelectedReadPhaseV1 {
    BeforeSelectedProjection,
    BeforeFinalProof,
}
#[path = "pr_checks_chosen_read_v1.rs"]
mod chosen_read;
#[path = "pr_checks_other_job_logs_v1.rs"]
mod other_job_logs;
pub use other_job_logs::PrCheckOtherJobLogsV1;
