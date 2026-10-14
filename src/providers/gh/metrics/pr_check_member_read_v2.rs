//! Undeclared PRIVATE shared exact member kernel for log and annotations paths.
use super::*;
use std::collections::BTreeSet;

pub(super) struct OpenedMemberV2 {
    pub(super) who: HostIdentity,
    pub(super) original: PrCheckOriginal,
    pub(super) repo: MetricsRepository,
    pub(super) parent_id: u64,
    pub(super) wanted: ObservedRun,
    pub(super) check: CheckProof,
    pub(super) job: Option<ActionsJob>,
    pub(super) job_node: Option<String>,
    pub(super) expected_job: u64,
    pub(super) coverage: PrCheckBridgePageCoverage,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn open_member_v2<'a>(
    provider: &GithubProvider,
    identity: &HostIdentity,
    current: &'a CurrentCatalogueRepository,
    resource: &'a PrResourceReadV1,
    index: usize,
    origin: &Arc<PrCheckRunObservation>,
    choice: &PrCheckChoiceV1,
    budget: &mut MetricsBudget,
    limits: &PrCheckRunReadLimits,
    cancel: &CancellationToken,
) -> GithubResult<(ScopedPrCheckBindingV1<'a>, OpenedMemberV2)> {
    if !choice.belongs_to(origin)
        || !provider.runner.same_instance(&origin.producer)
        || !origin.matches_resource_v1(identity, resource, index)
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Original opaque member/resource/producer mismatch; nothing launched",
        ));
    }
    let member = choice.job()?.ok_or_else(|| {
        error(
            GithubErrorKind::InvalidTarget,
            "Actual observed job choice required",
        )
    })?;
    let wanted = choice.run()?;
    let binding = ScopedPrCheckBindingV1::new(current, resource, index)?;
    let (summary, selected) = binding.summary_and_check();
    let original = PrCheckOriginal::new(provider, identity, &summary, &selected)?;
    let who = principal(provider, budget, identity, cancel)?;
    let base = format!("repos/{}", original.repository);
    let raw = budget.read_document(provider, &base, cancel)?;
    let repo = current_repo(provider, &original, &raw)?;
    binding.repository(&raw)?;
    // The genuine current catalogue row binds the owner. Only its closed
    // same-ID/node leaf rename may change navigation spelling across reads.
    if origin.repository.host != repo.host
        || origin.repository.id != repo.id
        || origin.repository.node_id != repo.node_id
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Original repository identity changed before member read",
        ));
    }
    let raw = budget.read_document(
        provider,
        &format!("{base}/pulls/{}", original.number),
        cancel,
    )?;
    let parent_id = current_parent(provider, &original, &repo, &raw)?;
    binding.parent(provider, &raw)?;
    if parent_id != origin.pr_database_id {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Original member PR database changed",
        ));
    }
    let check = CheckProof {
        node: origin.check_node.clone(),
        suite: origin.suite_id,
    };
    let path = format!("{base}/actions/runs/{}", wanted.run.id);
    let run = observed_run(
        provider,
        &original,
        &repo,
        &check,
        &budget.read_document(provider, &path, cancel)?,
    )?;
    same_run(&wanted, &run)?;
    let attempt_path = format!("{path}/attempts/{}", wanted.run.attempt);
    let attempt = observed_run(
        provider,
        &original,
        &repo,
        &check,
        &budget.read_document(provider, &attempt_path, cancel)?,
    )?;
    same_run(&run, &attempt)?;
    let endpoint = ExactAttemptEndpoint {
        repository: original.repository.clone(),
        run_id: wanted.run.id,
        attempt: wanted.run.attempt,
        head: original.head.clone(),
    };
    let mut raw_rows = 0;
    let (raw_jobs, coverage) = pages(
        provider,
        budget,
        limits,
        PageRequest {
            endpoint: &format!("{attempt_path}/jobs"),
            field: "jobs",
            max_pages: limits.max_job_pages,
        },
        &mut raw_rows,
        cancel,
    )?;
    let mut projection = JobProjectionBudget::new(limits);
    let mut ids = BTreeSet::new();
    let mut found = None;
    let mut job_node = None;
    for raw in &raw_jobs {
        let (job, _) = job_at(raw, &endpoint, &mut projection)?;
        if !ids.insert(job.id)
            || !exact_api_url(
                &provider.host,
                &format!("{base}/actions/jobs/{}", job.id),
                &raw["url"],
            )
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Actual member source/duplicate identity differs",
            ));
        }
        if job.id == member.id() {
            let observed = node(raw, "node_id")?;
            if observed != member.node() {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Original job node replaced; no numeric alias",
                ));
            }
            job_node = Some(observed);
            found = Some(job);
        }
    }
    Ok((
        binding,
        OpenedMemberV2 {
            who,
            original,
            repo,
            parent_id,
            wanted: attempt,
            check,
            job: found,
            job_node,
            expected_job: member.id(),
            coverage,
        },
    ))
}
pub(super) fn close_job_v2(
    provider: &GithubProvider,
    opened: &OpenedMemberV2,
    budget: &mut MetricsBudget,
    limits: &PrCheckRunReadLimits,
    cancel: &CancellationToken,
    require_check_link: bool,
) -> GithubResult<()> {
    if let Some(job) = &opened.job {
        let path = format!(
            "repos/{}/actions/jobs/{}",
            opened.original.repository, opened.expected_job
        );
        let raw = budget.read_document(provider, &path, cancel)?;
        let endpoint = ExactAttemptEndpoint {
            repository: opened.original.repository.clone(),
            run_id: opened.wanted.run.id,
            attempt: opened.wanted.run.attempt,
            head: opened.original.head.clone(),
        };
        let mut projection = JobProjectionBudget::new(limits);
        let (closing, _) = job_at(&raw, &endpoint, &mut projection)?;
        if closing.id != opened.expected_job
            || Some(node(&raw, "node_id")?).as_deref() != opened.job_node.as_deref()
            || !exact_api_url(&provider.host, &path, &raw["url"])
            || require_check_link && closing.check_run_url != job.check_run_url
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Actual original job/link changed after category read",
            ));
        }
    }
    Ok(())
}
pub(super) fn close_parent_v2(
    provider: &GithubProvider,
    opened: &OpenedMemberV2,
    binding: &ScopedPrCheckBindingV1<'_>,
    budget: &mut MetricsBudget,
    cancel: &CancellationToken,
) -> GithubResult<()> {
    let base = format!("repos/{}", opened.original.repository);
    let run = observed_run(
        provider,
        &opened.original,
        &opened.repo,
        &opened.check,
        &budget.read_document(
            provider,
            &format!("{base}/actions/runs/{}", opened.wanted.run.id),
            cancel,
        )?,
    )?;
    same_run(&opened.wanted, &run)?;
    let raw = budget.read_document(
        provider,
        &format!("{base}/pulls/{}", opened.original.number),
        cancel,
    )?;
    let parent = current_parent(provider, &opened.original, &opened.repo, &raw)?;
    binding.parent(provider, &raw)?;
    if parent != opened.parent_id {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Original PR changed after category read",
        ));
    }
    Ok(())
}
pub(super) fn close_account_v2(
    provider: &GithubProvider,
    opened: &OpenedMemberV2,
    binding: &ScopedPrCheckBindingV1<'_>,
    budget: &mut MetricsBudget,
    cancel: &CancellationToken,
) -> GithubResult<HostIdentity> {
    let raw = budget.read_document(
        provider,
        &format!("repos/{}", opened.original.repository),
        cancel,
    )?;
    let repo = current_repo(provider, &opened.original, &raw)?;
    binding.repository(&raw)?;
    require_same_repo(&opened.repo, &repo)?;
    principal(provider, budget, &opened.who, cancel)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn other_logs_v2(
    provider: &GithubProvider,
    identity: &HostIdentity,
    current: &CurrentCatalogueRepository,
    resource: &PrResourceReadV1,
    index: usize,
    origin: Arc<PrCheckRunObservation>,
    choice: &PrCheckChoiceV1,
    limits: &PrCheckLogReadLimits,
    run_limits: &PrCheckRunReadLimits,
    cancel: &CancellationToken,
    mut guard: impl FnMut(PrCheckLogReadPhase) -> GithubResult<()>,
) -> GithubResult<PrCheckOtherJobLogsV1> {
    let selected = limits.selected()?;
    run_limits.selected()?;
    let mut budget = MetricsBudget::new(&selected)?;
    let (binding, opened) = open_member_v2(
        provider,
        identity,
        current,
        resource,
        index,
        &origin,
        choice,
        &mut budget,
        run_limits,
        cancel,
    )?;
    let found = opened.job.is_some();
    let capture = if found {
        budget
            .pr_check_guard(cancel, || guard(PrCheckLogReadPhase::BeforeCapture))
            .and_then(|()| {
                budget.pr_check_log_capture(
                    provider,
                    vec![
                        "run".into(),
                        "view".into(),
                        opened.wanted.run.id.to_string(),
                        "--repo".into(),
                        format!("{}/{}", provider.host, opened.original.repository),
                        "--attempt".into(),
                        opened.wanted.run.attempt.to_string(),
                        "--log".into(),
                        "--job".into(),
                        opened.expected_job.to_string(),
                    ],
                    limits.max_log_bytes,
                    cancel,
                )
            })
    } else {
        Ok(String::new())
    };
    close_job_v2(provider, &opened, &mut budget, run_limits, cancel, false)?;
    close_parent_v2(provider, &opened, &binding, &mut budget, cancel)?;
    let local = budget.pr_check_guard(cancel, || guard(PrCheckLogReadPhase::BeforeFinalProof));
    let who = close_account_v2(provider, &opened, &binding, &mut budget, cancel)?;
    local.map_err(|e| error(e.kind, "Original other-job local guard unavailable"))?;
    let logs = capture.map_err(|e| {
        error(
            e.kind,
            "Other-job capture unavailable after real closing proof",
        )
    })?;
    if found && logs.lines().count() > limits.max_lines {
        return Err(error(
            GithubErrorKind::Unsupported,
            "Other-job log line cap reached",
        ));
    }
    let unavailable = (!found).then_some(if opened.coverage.enumeration_complete {
        PrCheckChoiceUnavailableV1::SelectedMemberNotObserved
    } else {
        PrCheckChoiceUnavailableV1::PartialMemberCatalogue
    });
    let read = if found {
        Some(logs::observation_from_closed_other_job_v1(
            origin,
            who.clone(),
            logs,
            limits,
            &budget,
            PrCheckCapturedTargetV2::member(choice)?,
        )?)
    } else {
        None
    };
    Ok(PrCheckOtherJobLogsV1::from_closed_v2(
        read,
        who,
        unavailable,
        found.then_some(opened.expected_job),
    ))
}
