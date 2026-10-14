//! SOURCE-ONLY pure worker projections, NOT authentication/access/history proofs.
//! Callers freshly guard original journals, principal, canonical access and publication.
use crate::{
    metrics::{
        health, CheckEvidence, CoverageState, EvidenceCoverage, MetricActor, MetricTarget,
        MetricsScope, PrTarget, RunEvidence,
    },
    providers::gh::{
        Actor, ExactPrCheckHealth, MetricsCheckObservation, MetricsPrObservation,
        MetricsReadCoverage, MetricsReadStatus, MetricsRepository, MetricsRequiredRules,
        MetricsRunObservation, RepositoryRunHealth,
    },
    search::model::valid_sha,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
};

const MAX_PARENTS: usize = 10_000;
const MAX_SOURCES: usize = 256;
const MAX_CHECKS: usize = 50_000;
const MAX_RUNS: usize = 10_000;
const MAX_METADATA: usize = 32 * 1024 * 1024;
const MAX_REASONS: usize = 128;

#[derive(Clone, Debug)]
pub struct CheckHealthProvenance {
    pub target: MetricTarget,
    pub observed: MetricsCheckObservation,
}
#[derive(Clone, Debug)]
pub struct CheckHealthSourceDetails {
    pub parent: PrTarget,
    pub check_runs: MetricsReadCoverage,
    pub commit_statuses: MetricsReadCoverage,
    pub required_rules: MetricsRequiredRules,
    pub source_limitations: Vec<String>,
    pub observed_from: u64,
    pub observed_through: u64,
    pub successful_response_bytes: usize,
    pub successful_diagnostic_bytes: usize,
    pub failed_reserved_bytes: usize,
}
#[derive(Clone, Debug)]
pub struct CheckHealthProjection {
    pub checks: Vec<CheckEvidence>,
    pub coverage: EvidenceCoverage,
    pub eligible_parents: usize,
    pub loaded_parents: Vec<PrTarget>,
    pub provenance: Vec<CheckHealthProvenance>,
    pub sources: Vec<CheckHealthSourceDetails>,
}
#[derive(Clone, Debug)]
pub struct RunHealthProvenance {
    pub target: MetricTarget,
    pub observed: MetricsRunObservation,
}
#[derive(Clone, Debug)]
pub struct RunHealthSourceDetails {
    pub repository: MetricsRepository,
    pub coverage: MetricsReadCoverage,
    pub source_limitations: Vec<String>,
    pub observed_from: u64,
    pub observed_through: u64,
    pub successful_response_bytes: usize,
    pub successful_diagnostic_bytes: usize,
    pub failed_reserved_bytes: usize,
}
#[derive(Clone, Debug)]
pub struct RunHealthProjection {
    pub runs: Vec<RunEvidence>,
    pub coverage: EvidenceCoverage,
    pub eligible_repositories: usize,
    pub loaded_repositories: Vec<MetricsRepository>,
    pub provenance: Vec<RunHealthProvenance>,
    pub sources: Vec<RunHealthSourceDetails>,
}

#[derive(Default)]
struct MetadataBudget {
    used: usize,
}
impl Write for MetadataBudget {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.used = self
            .used
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("Health projection metadata overflow"))?;
        if self.used > MAX_METADATA {
            return Err(io::Error::other(
                "Health projection metadata cap exceeded before cloning",
            ));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl MetadataBudget {
    fn serialized(&mut self, value: &impl Serialize) -> Result<(), String> {
        serde_json::to_writer(self, value).map_err(|_| {
            "Health projection exceeds bounded retained metadata before cloning".into()
        })
    }
    fn reasons(&mut self, reasons: &[String]) -> Result<(), String> {
        if reasons.len() > MAX_REASONS || reasons.iter().any(|reason| reason.len() > 4096) {
            return Err("Health source reasons exceed bounded metadata".into());
        }
        self.serialized(&reasons)
    }
    fn coverage(&mut self, coverage: &MetricsReadCoverage) -> Result<(), String> {
        self.reasons(&coverage.reasons)?;
        if let MetricsReadStatus::Unavailable(error) = &coverage.status {
            if error.message.len() > 4096 {
                return Err("Health source diagnostic exceeds bounded detail metadata".into());
            }
            self.serialized(&error.message)?;
        }
        Ok(())
    }
}
fn node(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn capture_budget(mut values: impl Iterator<Item = (usize, usize, usize)>) -> Result<(), String> {
    if values
        .try_fold(0usize, |used, (actual, diagnostic, reserved)| {
            used.checked_add(actual)
                .and_then(|used| used.checked_add(diagnostic))
                .and_then(|used| used.checked_add(reserved))
        })
        .is_none_or(|used| used > MAX_METADATA)
    {
        return Err(
            "Aggregate health source captures/reservations exceed worker projection budget".into(),
        );
    }
    Ok(())
}
fn repository(scope: &MetricsScope, value: &MetricsRepository) -> Result<(), String> {
    if value.host != scope.host()
        || value.id == 0
        || !node(&value.node_id)
        || value.full_name != value.full_name.to_ascii_lowercase()
        || !scope.accepts(&value.full_name)
    {
        return Err("Health repository differs from positive canonical explicit scope".into());
    }
    Ok(())
}
fn parent(scope: &MetricsScope, value: &MetricsPrObservation) -> Result<PrTarget, String> {
    repository(scope, &value.repository)?;
    if value.id == 0
        || value.number == 0
        || !node(&value.node_id)
        || value.head_sha.as_ref().is_none_or(|head| !valid_sha(head))
    {
        return Err(
            "Health parent needs original genuine database/node/number and full head".into(),
        );
    }
    let target = PrTarget {
        repository: value.repository.full_name.clone(),
        number: value.number,
        repository_id: Some(value.repository.id),
        database_id: Some(value.id),
        node_id: Some(value.node_id.clone()),
        source_record_id: None,
    };
    target.validate(scope)?;
    Ok(target)
}
fn same_parent(original: &MetricsPrObservation, fresh: &MetricsPrObservation) -> bool {
    original.repository == fresh.repository
        && original.id == fresh.id
        && original.node_id == fresh.node_id
        && original.number == fresh.number
        && original
            .head_sha
            .as_ref()
            .zip(fresh.head_sha.as_ref())
            .is_some_and(|(original, fresh)| {
                valid_sha(original) && valid_sha(fresh) && original.eq_ignore_ascii_case(fresh)
            })
        && original.state == fresh.state
        && original.draft == fresh.draft
        && original.created_at == fresh.created_at
        && original.merged_at == fresh.merged_at
        && original.closed_at == fresh.closed_at
        && original.merged == fresh.merged
        && original.observed_updated_at == fresh.observed_updated_at
        && original.requested_users == fresh.requested_users
        && original.requested_teams == fresh.requested_teams
}
fn eligible_coverage(
    value: &EvidenceCoverage,
    count: usize,
    budget: &mut MetadataBudget,
) -> Result<(), String> {
    if value.complete_empty && (count > 0 || !matches!(value.state, CoverageState::Complete)) {
        return Err("Eligible source empty proof contradicts explicit members/coverage".into());
    }
    match &value.state {
        CoverageState::Partial(reasons) => budget.reasons(reasons),
        CoverageState::Unavailable(reason) => {
            if reason.len() > 4096 {
                Err("Eligible source diagnostic exceeds metadata budget".into())
            } else {
                budget.serialized(reason)
            }
        }
        CoverageState::Complete => Ok(()),
    }
}
fn validate_coverage(
    value: &MetricsReadCoverage,
    loaded: usize,
    budget: &mut MetadataBudget,
) -> Result<(), String> {
    if value.admitted_rows != loaded
        || value.requested_pages > 100
        || value.fetched_pages > value.requested_pages
        || value.fetched_rows > 10_000
        || value.excluded_rows > value.fetched_rows
        || loaded.saturating_add(value.excluded_rows) > value.fetched_rows
        || value.response_bytes > MAX_METADATA
        || value.complete_empty
            != (value.enumeration_complete && value.fetched_rows == 0 && value.excluded_rows == 0)
        || value.enumeration_complete && value.fetched_pages == 0
        || matches!(value.status, MetricsReadStatus::Complete)
            && (!value.enumeration_complete || !value.metric_evidence_complete)
        || matches!(value.status, MetricsReadStatus::Unavailable(_)) && value.enumeration_complete
    {
        return Err(
            "Health source coverage contradicts actual loaded/capped/empty observations".into(),
        );
    }
    budget.coverage(value)
}
fn frame(
    interval: (u64, u64),
    accounting: (usize, usize, usize, usize, usize, usize),
    base: usize,
) -> Result<(), String> {
    let (from, through) = interval;
    let (requests, pages, rows, bytes, diagnostic, failed) = accounting;
    if from == 0
        || through < from
        || pages > 100
        || rows > 10_000
        || requests != base.saturating_add(pages)
        || bytes.saturating_add(diagnostic).saturating_add(failed) > MAX_METADATA
    {
        return Err("Health frame counters/UTC/capture reservations are not coherent bounded source evidence".into());
    }
    Ok(())
}
fn source_reason(coverage: &mut EvidenceCoverage, reason: String) {
    let count = match &coverage.state {
        CoverageState::Partial(reasons) => reasons.len(),
        _ => 0,
    };
    if count < 64 {
        coverage.partial(reason);
    } else if count == 64 {
        coverage.partial("Additional health-source limitations omitted from bounded summary; inspect exact source details");
    }
}
fn fold(coverage: &mut EvidenceCoverage, source: &MetricsReadCoverage, label: &str) {
    match &source.status {
        MetricsReadStatus::Unavailable(error) => {
            source_reason(coverage, format!("{label} unavailable: {error}"))
        }
        MetricsReadStatus::Partial => {
            source_reason(coverage, format!("{label} is a bounded partial source"))
        }
        MetricsReadStatus::Complete => {}
    }
    if !source.enumeration_complete || !source.metric_evidence_complete {
        source_reason(
            coverage,
            format!("{label} enumeration/health metadata incomplete"),
        );
    }
    for reason in &source.reasons {
        source_reason(coverage, format!("{label}: {reason}"));
    }
}
fn initial(eligible: &EvidenceCoverage) -> EvidenceCoverage {
    let mut value = EvidenceCoverage {
        state: CoverageState::Complete,
        complete_empty: false,
        fetched_at: None,
    };
    match &eligible.state {
        CoverageState::Partial(reasons) => {
            for reason in reasons {
                source_reason(&mut value, format!("Eligible source: {reason}"));
            }
        }
        CoverageState::Unavailable(reason) => source_reason(
            &mut value,
            format!("Eligible denominator unavailable: {reason}"),
        ),
        CoverageState::Complete => {}
    }
    value
}
fn validate_actor(value: Option<&crate::providers::gh::MetricsActor>) -> Result<(), String> {
    if value.is_some_and(|value| {
        value.account_id == Some(0)
            || [
                Some(&value.actor.login),
                Some(&value.actor.actor_type),
                value.actor.display_name.as_ref(),
                value.actor.avatar_url.as_ref(),
            ]
            .into_iter()
            .flatten()
            .any(|text| text.len() > 4096)
    }) {
        return Err("Health actor has a nonpositive supplied identity".into());
    }
    Ok(())
}
fn actor(value: Option<&crate::providers::gh::MetricsActor>) -> Result<MetricActor, String> {
    validate_actor(value)?;
    Ok(value.map_or(
        MetricActor {
            account_id: None,
            actor: Actor::default(),
        },
        |value| MetricActor {
            account_id: value.account_id,
            actor: value.actor.clone(),
        },
    ))
}

/// Pure metadata projection only. `eligible` is the explicit original parent set,
/// never inferred from whichever sources happened to finish or personal queues.
pub fn project_check_health(
    scope: &MetricsScope,
    eligible: &[MetricsPrObservation],
    eligible_source: &EvidenceCoverage,
    sources: &[ExactPrCheckHealth],
) -> Result<CheckHealthProjection, String> {
    scope.validate()?;
    if eligible.len() > MAX_PARENTS
        || sources.len() > MAX_SOURCES
        || sources
            .iter()
            .try_fold(0usize, |count, source| {
                count
                    .checked_add(source.check_runs.len())
                    .and_then(|count| count.checked_add(source.commit_statuses.len()))
            })
            .is_none_or(|count| count > MAX_CHECKS)
    {
        return Err("Check health projection exceeds explicit parent/source/record limits".into());
    }
    let mut budget = MetadataBudget::default();
    capture_budget(sources.iter().map(|source| {
        (
            source.aggregate_response_bytes,
            source.successful_diagnostic_bytes,
            source.failed_reserved_bytes,
        )
    }))?;
    eligible_coverage(eligible_source, eligible.len(), &mut budget)?;
    let mut expected = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    for original in eligible {
        let target = parent(scope, original)?;
        if original
            .requested_users
            .as_ref()
            .is_some_and(|users| users.len() > MAX_PARENTS)
            || original
                .requested_teams
                .as_ref()
                .is_some_and(|teams| teams.len() > MAX_PARENTS)
            || expected.insert(target.clone(), original).is_some()
            || !ids.insert(original.id)
            || !nodes.insert(original.node_id.clone())
        {
            return Err(
                "Explicit eligible health parents contradict genuine identities or bounds".into(),
            );
        }
        budget.serialized(original)?;
    }
    // Preflight all source counters, metadata and exact original identity before
    // cloning sidecars or producing display rows. No private read authority minted.
    let mut admitted = BTreeSet::new();
    for source in sources {
        let target = parent(scope, &source.parent)?;
        let original = expected
            .get(&target)
            .ok_or("Check source parent is not explicitly eligible")?;
        if !scope.matches(&source.identity)
            || !same_parent(original, &source.parent)
            || !admitted.insert(target)
        {
            return Err(
                "Check source principal/canonical parent/full head/freshness changed or repeated"
                    .into(),
            );
        }
        frame(
            (source.observed_from, source.observed_through),
            (
                source.requests,
                source.collection_pages,
                source.raw_rows,
                source.aggregate_response_bytes,
                source.successful_diagnostic_bytes,
                source.failed_reserved_bytes,
            ),
            6,
        )?;
        if source.collection_pages
            != source
                .check_run_coverage
                .requested_pages
                .saturating_add(source.commit_status_coverage.requested_pages)
            || source.raw_rows
                != source
                    .check_run_coverage
                    .fetched_rows
                    .saturating_add(source.commit_status_coverage.fetched_rows)
            || source.aggregate_response_bytes
                < source
                    .check_run_coverage
                    .response_bytes
                    .saturating_add(source.commit_status_coverage.response_bytes)
        {
            return Err("Check source endpoint counters contradict shared budget evidence".into());
        }
        validate_coverage(
            &source.check_run_coverage,
            source.check_runs.len(),
            &mut budget,
        )?;
        validate_coverage(
            &source.commit_status_coverage,
            source.commit_statuses.len(),
            &mut budget,
        )?;
        budget.reasons(&source.source_limitations)?;
        let MetricsRequiredRules::Unavailable { reason } = &source.required_rules;
        budget.reasons(std::slice::from_ref(reason))?;
        for (kind, observed) in source
            .check_runs
            .iter()
            .map(|observed| ("check", observed))
            .chain(
                source
                    .commit_statuses
                    .iter()
                    .map(|observed| ("status", observed)),
            )
        {
            if observed.id == 0
                || !valid_sha(&observed.head_sha)
                || !source
                    .parent
                    .head_sha
                    .as_ref()
                    .is_some_and(|head| head.eq_ignore_ascii_case(&observed.head_sha))
                || observed.node_id.as_ref().is_some_and(|value| !node(value))
                || observed.app_id == Some(0)
                || observed.name.as_ref().is_some_and(|name| name.len() > 4096)
                || observed.url.as_ref().is_some_and(|url| url.len() > 4096)
            {
                return Err("Check row canonical identity or raw bounded metadata is invalid before projection".into());
            }
            if observed.result.kind() != kind
                || observed.result.health_status().len() > 4096
                || observed.result.health_conclusion().len() > 4096
                || matches!(&observed.result,crate::providers::gh::MetricsCheckResult::CommitStatus{state:crate::providers::gh::MetricsCommitStatusState::Unknown(Some(value))} if value.len()>4096)
            {
                return Err("Health row source-kind identity changed".into());
            }
            budget.reasons(&observed.reasons)?;
            budget.serialized(observed)?;
            validate_actor(observed.actor.as_ref())?;
        }
    }
    let mut coverage = initial(eligible_source);
    let mut checks = vec![];
    let mut provenance = vec![];
    let mut details = vec![];
    let mut seen = BTreeSet::new();
    let mut opaque_nodes = BTreeMap::new();
    let mut any_available = false;
    for source in sources {
        let pr = parent(scope, &source.parent)?;
        coverage.fetched_at = Some(coverage.fetched_at.map_or(source.observed_through, |old| {
            old.min(source.observed_through)
        }));
        fold(&mut coverage, &source.check_run_coverage, "Check runs");
        fold(
            &mut coverage,
            &source.commit_status_coverage,
            "Commit statuses",
        );
        any_available |= source.check_run_coverage.fetched_pages > 0
            || source.commit_status_coverage.fetched_pages > 0;
        for reason in &source.source_limitations {
            source_reason(&mut coverage, reason.clone());
        }
        let MetricsRequiredRules::Unavailable { reason } = &source.required_rules;
        source_reason(&mut coverage, reason.clone());
        for (kind, values) in [
            ("check", &source.check_runs),
            ("status", &source.commit_statuses),
        ] {
            for observed in values {
                if observed.id == 0
                    || observed.result.kind() != kind
                    || !valid_sha(&observed.head_sha)
                    || !source
                        .parent
                        .head_sha
                        .as_ref()
                        .is_some_and(|head| head.eq_ignore_ascii_case(&observed.head_sha))
                    || observed
                        .node_id
                        .as_ref()
                        .is_some_and(|node_id| !node(node_id))
                    || observed.app_id == Some(0)
                {
                    return Err("Check observation lacks genuine exact source kind/ID/full parent head/node".into());
                }
                let target = MetricTarget::Check {
                    pr: pr.clone(),
                    head_sha: observed.head_sha.to_ascii_lowercase(),
                    check_id: observed.id,
                    kind: kind.into(),
                };
                target.validate(scope)?;
                if !seen.insert(target.clone()) {
                    return Err(
                        "Repeated exact check destination cannot be silently counted twice".into(),
                    );
                }
                if let Some(node) = &observed.node_id {
                    let key = (source.parent.repository.id, kind, observed.id);
                    if opaque_nodes
                        .insert(node.clone(), key)
                        .is_some_and(|old| old != key)
                    {
                        return Err(
                            "Genuine health opaque node contradicts type/database identity".into(),
                        );
                    }
                }
                checks.push(CheckEvidence {
                    target: target.clone(),
                    name: observed
                        .name
                        .clone()
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| format!("Observed {kind} — name unavailable")),
                    health: health(observed.health_status(), observed.health_conclusion()),
                });
                provenance.push(CheckHealthProvenance {
                    target,
                    observed: observed.clone(),
                });
            }
        }
        details.push(CheckHealthSourceDetails {
            parent: pr,
            check_runs: source.check_run_coverage.clone(),
            commit_statuses: source.commit_status_coverage.clone(),
            required_rules: source.required_rules.clone(),
            source_limitations: source.source_limitations.clone(),
            observed_from: source.observed_from,
            observed_through: source.observed_through,
            successful_response_bytes: source.aggregate_response_bytes,
            successful_diagnostic_bytes: source.successful_diagnostic_bytes,
            failed_reserved_bytes: source.failed_reserved_bytes,
        });
    }
    for target in expected.keys().filter(|target| !admitted.contains(*target)) {
        source_reason(
            &mut coverage,
            format!(
                "Eligible {}#{} exact-head checks not loaded",
                target.repository, target.number
            ),
        );
    }
    source_reason(&mut coverage,format!("Loaded exact-head sources for{} of{} explicitly eligible parents; denominator completeness is supplied separately",admitted.len(),eligible.len()));
    for reason in [
        "Required branch/ruleset checks and merge eligibility remain unavailable",
        "Check API is limited to1,000 latest suites; fork association/erased checks may be missing",
        "API-visible current health is non-atomic, not historical or required-rule completeness",
    ] {
        source_reason(&mut coverage, reason.into());
    }
    if !any_available && checks.is_empty() {
        coverage = EvidenceCoverage::unavailable(
            "No available observed exact-head health source; missing data is not zero",
        );
    }
    Ok(CheckHealthProjection {
        checks,
        coverage,
        eligible_parents: eligible.len(),
        loaded_parents: admitted.into_iter().collect(),
        provenance,
        sources: details,
    })
}

pub fn project_run_health(
    scope: &MetricsScope,
    eligible: &[MetricsRepository],
    eligible_source: &EvidenceCoverage,
    sources: &[RepositoryRunHealth],
) -> Result<RunHealthProjection, String> {
    scope.validate()?;
    if eligible.len() > 64
        || sources.len() > 64
        || sources
            .iter()
            .try_fold(0usize, |count, source| count.checked_add(source.runs.len()))
            .is_none_or(|count| count > MAX_RUNS)
    {
        return Err(
            "Run health projection exceeds explicit repository/source/record bounds".into(),
        );
    }
    let mut budget = MetadataBudget::default();
    capture_budget(sources.iter().map(|source| {
        (
            source.aggregate_response_bytes,
            source.successful_diagnostic_bytes,
            source.failed_reserved_bytes,
        )
    }))?;
    eligible_coverage(eligible_source, eligible.len(), &mut budget)?;
    let mut expected = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    for repo in eligible {
        repository(scope, repo)?;
        budget.serialized(repo)?;
        if expected.insert(repo.full_name.clone(), repo).is_some()
            || !ids.insert(repo.id)
            || !nodes.insert(repo.node_id.clone())
        {
            return Err("Eligible canonical health repositories contradict identity".into());
        }
    }
    let mut admitted = BTreeSet::new();
    for source in sources {
        repository(scope, &source.repository)?;
        if !scope.matches(&source.identity)
            || expected
                .get(&source.repository.full_name)
                .is_none_or(|repo| **repo != source.repository)
            || !admitted.insert(source.repository.full_name.clone())
        {
            return Err(
                "Run health source principal/canonical eligibility changed or repeated".into(),
            );
        }
        frame(
            (source.observed_from, source.observed_through),
            (
                source.requests,
                source.collection_pages,
                source.raw_rows,
                source.aggregate_response_bytes,
                source.successful_diagnostic_bytes,
                source.failed_reserved_bytes,
            ),
            4,
        )?;
        if source.collection_pages != source.coverage.requested_pages
            || source.raw_rows != source.coverage.fetched_rows
            || source.aggregate_response_bytes < source.coverage.response_bytes
        {
            return Err("Run source counters contradict actual shared source budget".into());
        }
        validate_coverage(&source.coverage, source.runs.len(), &mut budget)?;
        budget.reasons(&source.source_limitations)?;
        for observed in &source.runs {
            if observed.repository != source.repository
                || observed.id == 0
                || observed.workflow_id == 0
                || observed.attempt == 0
                || !valid_sha(&observed.head_sha)
                || !node(&observed.node_id)
                || observed
                    .status
                    .api_value()
                    .is_some_and(|value| value.len() > 4096)
                || observed
                    .conclusion
                    .as_ref()
                    .is_some_and(|value| value.api_value().len() > 4096)
                || [
                    &observed.name,
                    &observed.title,
                    &observed.event,
                    &observed.head_branch,
                    &observed.url,
                ]
                .into_iter()
                .flatten()
                .any(|text| text.len() > 4096)
            {
                return Err("Run row canonical identity or raw bounded metadata is invalid before projection".into());
            }
            budget.reasons(&observed.reasons)?;
            budget.serialized(observed)?;
            validate_actor(observed.actor.as_ref())?;
            validate_actor(observed.triggering_actor.as_ref())?;
        }
    }
    let mut coverage = initial(eligible_source);
    let mut runs = vec![];
    let mut provenance = vec![];
    let mut details = vec![];
    let mut seen = BTreeSet::new();
    let mut opaque_nodes = BTreeMap::new();
    let mut any_available = false;
    for source in sources {
        coverage.fetched_at = Some(coverage.fetched_at.map_or(source.observed_through, |old| {
            old.min(source.observed_through)
        }));
        fold(&mut coverage, &source.coverage, "Workflow runs");
        any_available |= source.coverage.fetched_pages > 0;
        for reason in &source.source_limitations {
            source_reason(&mut coverage, reason.clone());
        }
        for observed in &source.runs {
            if observed.repository != source.repository
                || observed.id == 0
                || observed.workflow_id == 0
                || observed.attempt == 0
                || !valid_sha(&observed.head_sha)
                || !node(&observed.node_id)
                || !seen.insert((source.repository.id, observed.id))
            {
                return Err("Run health row contradicts genuine current canonical run/workflow/attempt/head".into());
            }
            if opaque_nodes
                .insert(
                    observed.node_id.clone(),
                    (source.repository.id, observed.id),
                )
                .is_some_and(|old| old != (source.repository.id, observed.id))
            {
                return Err("Actual run opaque node contradicts positive database identity".into());
            }
            let target = MetricTarget::Run {
                repository: source.repository.full_name.clone(),
                repository_id: Some(source.repository.id),
                run_id: observed.id,
                attempt: observed.attempt,
                workflow_id: observed.workflow_id,
                head_sha: observed.head_sha.to_ascii_lowercase(),
            };
            target.validate(scope)?;
            runs.push(RunEvidence {
                target: target.clone(),
                title: observed
                    .title
                    .clone()
                    .filter(|title| !title.is_empty())
                    .or_else(|| observed.name.clone().filter(|name| !name.is_empty()))
                    .unwrap_or_else(|| "Observed run — title unavailable".into()),
                actor: actor(observed.triggering_actor.as_ref())?,
                created_at: observed.created_at.as_ref().map(|date| date.unix_seconds()),
                observed_at: source.observed_through,
                health: if matches!(
                    observed.status,
                    crate::providers::gh::MetricsHealthStatus::Unknown(_)
                ) || matches!(
                    observed.conclusion,
                    Some(crate::providers::gh::MetricsHealthConclusion::Unknown(_))
                ) {
                    crate::metrics::Health::Unknown
                } else {
                    health(
                        observed.status.api_value().unwrap_or("unknown"),
                        observed
                            .conclusion
                            .as_ref()
                            .map_or("", |conclusion| conclusion.api_value()),
                    )
                },
            });
            provenance.push(RunHealthProvenance {
                target,
                observed: observed.clone(),
            });
        }
        details.push(RunHealthSourceDetails {
            repository: source.repository.clone(),
            coverage: source.coverage.clone(),
            source_limitations: source.source_limitations.clone(),
            observed_from: source.observed_from,
            observed_through: source.observed_through,
            successful_response_bytes: source.aggregate_response_bytes,
            successful_diagnostic_bytes: source.successful_diagnostic_bytes,
            failed_reserved_bytes: source.failed_reserved_bytes,
        });
    }
    for missing in expected.keys().filter(|name| !admitted.contains(*name)) {
        source_reason(
            &mut coverage,
            format!("Eligible {missing} current run source not loaded"),
        );
    }
    source_reason(
        &mut coverage,
        format!(
            "Loaded current sources for{} of{} explicitly eligible repositories",
            admitted.len(),
            eligible.len()
        ),
    );
    for reason in ["Highest OBSERVED attempt per actual run; no atomic latest/current or historical coverage claim","Run created_at defines UTC cohort; updated_at is not completion/duration","Required rules, workflow definitions and jobs are not loaded by observed health"] {source_reason(&mut coverage,reason.into());}
    if !any_available && runs.is_empty() {
        coverage = EvidenceCoverage::unavailable(
            "No available observed workflow health source; missing data is not zero",
        );
    }
    Ok(RunHealthProjection {
        runs,
        coverage,
        eligible_repositories: eligible.len(),
        loaded_repositories: eligible
            .iter()
            .filter(|repo| admitted.contains(&repo.full_name))
            .cloned()
            .collect(),
        provenance,
        sources: details,
    })
}

#[cfg(test)]
#[path = "metrics_health_tests.rs"]
mod tests;
