//! SOURCE-ONLY additive observed check/run health; no required-rules or duration proof.
//! Root owns original journal/cache authority, total worker budgets and explicit loading.
use super::*;
mod budget;
mod model;
mod paging;
mod parse;
use budget::HealthTransport;
pub use model::*;
use paging::*;
use parse::*;

fn canonical(
    provider: &GithubProvider,
    identity: &HostIdentity,
    repo: &MetricsRepository,
) -> GithubResult<()> {
    scope(provider, identity, &repo.full_name)?;
    if repo.host != provider.host
        || repo.full_name != repo.full_name.to_ascii_lowercase()
        || repo.id == 0
        || !valid_opaque(&repo.node_id)
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Health source needs exact positive canonical host/repository database/node/name",
        ));
    }
    Ok(())
}
fn status_contexts(values: &mut [MetricsCheckObservation], coverage: &mut MetricsReadCoverage) {
    let mut counts = BTreeMap::<String, usize>::new();
    for value in values.iter() {
        if let Some(name) = &value.name {
            *counts.entry(name.clone()).or_default() += 1;
        }
    }
    for value in values.iter_mut().filter(|value| {
        value
            .name
            .as_ref()
            .is_none_or(|name| counts.get(name).is_some_and(|count| *count > 1))
    }) {
        value.current_context_unique = false;
        value.reasons.push("Combined status repeated exact context; current-context selection unavailable, no name/case alias chosen".into());
        coverage.metric_evidence_complete = false;
        coverage.reasons.extend(value.reasons.clone());
    }
    finish(coverage);
}
fn check_nodes(
    checks: &[MetricsCheckObservation],
    statuses: &[MetricsCheckObservation],
) -> GithubResult<()> {
    let mut nodes = BTreeMap::<String, (String, u64)>::new();
    for value in checks.iter().chain(statuses) {
        if let Some(node) = &value.node_id {
            let key = (value.result.kind().to_owned(), value.id);
            if nodes
                .insert(node.clone(), key.clone())
                .is_some_and(|old| old != key)
            {
                return Err(error(GithubErrorKind::IdentityChanged, "Actual opaque check/status node contradicts positive type/database membership; no ID alias guessed"));
            }
        }
    }
    Ok(())
}
fn highest_observed(
    values: Vec<MetricsRunObservation>,
    coverage: &mut MetricsReadCoverage,
) -> GithubResult<Vec<MetricsRunObservation>> {
    let mut runs = BTreeMap::<u64, MetricsRunObservation>::new();
    let mut nodes = BTreeMap::<String, u64>::new();
    for value in values {
        if nodes
            .insert(value.node_id.clone(), value.id)
            .is_some_and(|id| id != value.id)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Actual run database/node identities contradict; no opaque/numeric alias guessed",
            ));
        }
        if let Some(old) = runs.get(&value.id) {
            if old.repository != value.repository
                || old.node_id != value.node_id
                || old.workflow_id != value.workflow_id
                || old.head_sha != value.head_sha
                || old.created_at != value.created_at
            {
                return Err(error(GithubErrorKind::IdentityChanged,"Run canonical workflow/head/original creation changed across observed attempts"));
            }
            coverage.metric_evidence_complete = false;
            coverage.enumeration_complete = false;
            coverage.reasons.push("Run repeated with different observed attempt while paging; retain highest OBSERVED attempt only, not complete atomic inventory".into());
            if value.attempt <= old.attempt {
                continue;
            }
        }
        runs.insert(value.id, value);
    }
    coverage.admitted_rows = runs.len();
    finish(coverage);
    Ok(runs.into_values().collect())
}
impl GithubProvider {
    /// Own fresh user/repository/PR/check runs/combined statuses/repository/PR/user
    /// (minimum8) under one shared pages/rows/bytes/time/cancel budget.
    pub fn metrics_pr_check_health(
        &self,
        identity: &HostIdentity,
        original: &MetricsPrObservation,
        limits: &MetricsReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<ExactPrCheckHealth> {
        canonical(self, identity, &original.repository)?;
        limits.validate()?;
        if original.id == 0
            || original.number == 0
            || !valid_opaque(&original.node_id)
            || original
                .head_sha
                .as_ref()
                .is_none_or(|sha| !crate::search::model::valid_sha(sha))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "PR check health needs original positive canonical PR identity and full40/64 head",
            ));
        }
        let mut budget = HealthBudget::new(limits)?;
        budget.principal(self, identity, cancel)?;
        let endpoint = format!("repos/{}", original.repository.full_name);
        let parent_endpoint = format!("{endpoint}/pulls/{}", original.number);
        let repo = repository(
            &self.host,
            &original.repository.full_name,
            &budget.transport.read_document(self, &endpoint, cancel)?,
        )?;
        require_same_repo(&original.repository, &repo)?;
        let parent = parse_pr(
            &repo,
            &budget
                .transport
                .read_document(self, &parent_endpoint, cancel)?,
        )?;
        require_same_parent(original, &parent)?;
        let head = parent.head_sha.as_ref().ok_or_else(|| {
            error(
                GithubErrorKind::StaleHead,
                "Fresh exact PR head unavailable",
            )
        })?;
        let (check_runs, check_run_coverage) = budget.collect(
            self,
            &format!("{endpoint}/commits/{head}/check-runs?filter=latest"),
            "check_runs",
            |_| Ok(()),
            |raw| {
                let item = check(raw, &parent)?;
                Ok((
                    format!("check:{}", item.id),
                    item.clone(),
                    item.reasons.clone(),
                ))
            },
            cancel,
        )?;
        let (mut commit_statuses, mut commit_status_coverage) = budget.collect(
            self,
            &format!("{endpoint}/commits/{head}/status"),
            "statuses",
            |raw| status_header(raw, &parent),
            |raw| {
                let item = commit_status(raw, &parent)?;
                Ok((
                    format!("status:{}", item.id),
                    item.clone(),
                    item.reasons.clone(),
                ))
            },
            cancel,
        )?;
        status_contexts(&mut commit_statuses, &mut commit_status_coverage);
        check_nodes(&check_runs, &commit_statuses)?;
        let final_repo = repository(
            &self.host,
            &repo.full_name,
            &budget.transport.read_document(self, &endpoint, cancel)?,
        )?;
        require_same_repo(&repo, &final_repo)?;
        let final_parent = parse_pr(
            &repo,
            &budget
                .transport
                .read_document(self, &parent_endpoint, cancel)?,
        )?;
        require_same_parent(&parent, &final_parent)?;
        budget.principal(self, identity, cancel)?;
        Ok(ExactPrCheckHealth {
            identity:identity.clone(),parent:final_parent,check_runs,commit_statuses,check_run_coverage,commit_status_coverage,
            required_rules:MetricsRequiredRules::Unavailable{reason:"Required branch protection/ruleset checks and merge eligibility are not loaded; observed check failure is not a certified required blocker".into()},
            source_limitations:vec!["Current API-visible latest check runs/status contexts only; non-atomic observations, not complete historical checks".into(),"Commit check endpoint is limited to the1,000 most recent check suites; fork push association can be unavailable".into()],
            observed_from:budget.transport.observed_from,observed_through:budget.transport.observed_through()?,requests:budget.transport.requests,raw_rows:budget.raw_rows,collection_pages:budget.collection_pages,aggregate_response_bytes:budget.transport.bytes,successful_diagnostic_bytes:budget.transport.successful_diagnostic_bytes,failed_reserved_bytes:budget.transport.failed_reserved_bytes,
        })
    }
    /// Own fresh user/repository/unfiltered run pages/repository/user (minimum5).
    /// No implicit window search filter, all attempts, jobs or completion durations.
    pub fn metrics_repository_run_health(
        &self,
        identity: &HostIdentity,
        expected: &MetricsRepository,
        limits: &MetricsReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<RepositoryRunHealth> {
        canonical(self, identity, expected)?;
        limits.validate()?;
        let mut budget = HealthBudget::new(limits)?;
        budget.principal(self, identity, cancel)?;
        let endpoint = format!("repos/{}", expected.full_name);
        let repo = repository(
            &self.host,
            &expected.full_name,
            &budget.transport.read_document(self, &endpoint, cancel)?,
        )?;
        require_same_repo(expected, &repo)?;
        let (runs, mut coverage) = budget.collect(
            self,
            &format!("{endpoint}/actions/runs"),
            "workflow_runs",
            |_| Ok(()),
            |raw| {
                let item = run(raw, &repo)?;
                Ok((
                    format!("run:{}:attempt:{}", item.id, item.attempt),
                    item.clone(),
                    item.reasons.clone(),
                ))
            },
            cancel,
        )?;
        let runs = highest_observed(runs, &mut coverage)?;
        let final_repo = repository(
            &self.host,
            &repo.full_name,
            &budget.transport.read_document(self, &endpoint, cancel)?,
        )?;
        require_same_repo(&repo, &final_repo)?;
        budget.principal(self, identity, cancel)?;
        Ok(RepositoryRunHealth {
            identity:identity.clone(),repository:repo,runs,coverage,
            source_limitations:vec!["Unfiltered current repository run catalogue; highest OBSERVED attempt per actual run, no atomic latest/current guarantee or historical trends".into(),"Workflow definitions/jobs/required rules not loaded; run updated_at is not completion or duration".into()],
            observed_from:budget.transport.observed_from,observed_through:budget.transport.observed_through()?,requests:budget.transport.requests,raw_rows:budget.raw_rows,collection_pages:budget.collection_pages,aggregate_response_bytes:budget.transport.bytes,successful_diagnostic_bytes:budget.transport.successful_diagnostic_bytes,failed_reserved_bytes:budget.transport.failed_reserved_bytes,
        })
    }
}
