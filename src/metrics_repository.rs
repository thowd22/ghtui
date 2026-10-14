//! Typed repository-wide dashboard projection. Workers separately establish
//! fresh principal/access/journal authority; a DTO is not a historical proof.
#[path = "metrics_repository_read_guard.rs"]
mod read_guard;
use crate::{
    metrics::{
        CoverageState, DashboardEvidence, EventCollection, EvidenceCoverage, MetricActor,
        MetricsScope, PrCohortKind, PrEvidence, PrState, PrTarget,
    },
    providers::gh::{
        HostIdentity, MetricsPrObservation, MetricsPrState, MetricsReadStatus,
        RepositoryPrInventory,
    },
};
pub use read_guard::MetricsReadGuard;
use std::collections::BTreeSet;

const MAX_PRS: usize = 10_000;
const MAX_METADATA: usize = 32 * 1024 * 1024;

pub fn repository_evidence(
    identity: &HostIdentity,
    declared: &[String],
    inventories: &[RepositoryPrInventory],
) -> Result<DashboardEvidence, String> {
    let scope = MetricsScope::new(identity, declared)?;
    if inventories.len() > 64
        || inventories
            .iter()
            .map(|source| source.prs.len())
            .sum::<usize>()
            > MAX_PRS
    {
        return Err("Dashboard inventory exceeds the selected-repository/PR budget".into());
    }
    let mut metadata_bytes = 0usize;
    let mut admitted = BTreeSet::new();
    let mut parents = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    let mut database_ids = BTreeSet::new();
    let mut repository_ids = BTreeSet::new();
    let mut prs = vec![];
    let mut coverage = EvidenceCoverage {
        state: CoverageState::Complete,
        complete_empty: true,
        fetched_at: None,
    };
    for inventory in inventories {
        let repository = &inventory.repository;
        if !scope.matches(&inventory.identity)
            || !repository.host.eq_ignore_ascii_case(scope.host())
            || !scope.accepts(&repository.full_name)
            || repository.id == 0
            || !valid_node(&repository.node_id)
            || !admitted.insert(repository.full_name.to_ascii_lowercase())
            || !repository_ids.insert(repository.id)
            || inventory.observed_through < inventory.observed_from
            || inventory.coverage.admitted_rows != inventory.prs.len()
        {
            return Err("Dashboard inventory identity, explicit scope or source metadata changed; refresh without retargeting".into());
        }
        coverage.fetched_at = Some(
            coverage
                .fetched_at
                .map_or(inventory.observed_through, |old| {
                    old.min(inventory.observed_through)
                }),
        );
        match &inventory.coverage.status {
            MetricsReadStatus::Unavailable(error) => {
                if !inventory.prs.is_empty() {
                    return Err("An unavailable repository cannot admit metric records".into());
                }
                coverage.partial(format!("{} unavailable: {error}", repository.full_name));
            }
            MetricsReadStatus::Partial => coverage.partial(format!(
                "{} is a bounded partial inventory",
                repository.full_name
            )),
            MetricsReadStatus::Complete => {}
        }
        if !inventory.coverage.enumeration_complete || !inventory.coverage.metric_evidence_complete
        {
            coverage.partial(format!(
                "{} enumeration or metric metadata is incomplete",
                repository.full_name
            ));
        }
        for reason in &inventory.coverage.reasons {
            budget_text(&mut metadata_bytes, reason, 4096)?;
            source_reason(&mut coverage, format!("{}: {reason}", repository.full_name));
        }
        coverage.complete_empty &= inventory.coverage.complete_empty && inventory.prs.is_empty();
        if inventory.coverage.complete_empty && !inventory.prs.is_empty() {
            return Err("Complete-empty repository proof contradicts loaded PRs".into());
        }
        for pr in &inventory.prs {
            if &pr.repository != repository
                || pr.id == 0
                || pr.number == 0
                || !valid_node(&pr.node_id)
                || !parents.insert((repository.id, pr.number))
                || !nodes.insert(pr.node_id.clone())
                || !database_ids.insert(pr.id)
            {
                return Err(
                    "Canonical Dashboard PR identity contradicts its original repository/parent"
                        .into(),
                );
            }
            // Preflight retained metadata before cloning contributor rows. API
            // response byte limits alone do not bound multiple repositories.
            budget_text(&mut metadata_bytes, &pr.title, 256 * 1024)?;
            budget_text(&mut metadata_bytes, &pr.node_id, 1024)?;
            budget_actor(&mut metadata_bytes, &pr.author)?;
            for reason in &pr.reasons {
                budget_text(&mut metadata_bytes, reason, 4096)?;
                source_reason(
                    &mut coverage,
                    format!("{}#{}: {reason}", repository.full_name, pr.number),
                );
            }
            if let Some(users) = &pr.requested_users {
                if users.len() > MAX_PRS {
                    return Err("Requested-user metadata exceeds Dashboard budget".into());
                }
                for user in users {
                    budget_actor(&mut metadata_bytes, user)?;
                }
            }
            if metadata_bytes > MAX_METADATA {
                return Err("Dashboard retained metadata byte budget exceeded".into());
            }
            prs.push(project_pr(&scope, pr)?);
        }
    }
    for missing in scope.repositories().difference(&admitted) {
        coverage.partial(format!(
            "{missing} has no verified repository-wide inventory"
        ));
    }
    if inventories.is_empty() {
        coverage =
            EvidenceCoverage::unavailable("No verified selected-repository inventory loaded");
    }
    Ok(DashboardEvidence {
        scope,
        cohort: PrCohortKind::SelectedRepositoryInventory,
        prs,
        pr_coverage: coverage,
        checks: vec![],
        check_coverage: EvidenceCoverage::unavailable(
            "Exact-head check sources have not been loaded for this repository cohort",
        ),
        runs: vec![],
        run_coverage: EvidenceCoverage::unavailable(
            "Repository/window Actions sources have not been loaded",
        ),
    })
}

fn budget_text(used: &mut usize, text: &str, limit: usize) -> Result<(), String> {
    *used = used.saturating_add(text.len());
    if text.len() > limit || *used > MAX_METADATA {
        return Err("Dashboard retained metadata byte budget exceeded".into());
    }
    Ok(())
}

fn budget_actor(
    used: &mut usize,
    actor: &crate::providers::gh::MetricsActor,
) -> Result<(), String> {
    if actor.account_id == Some(0) {
        return Err("Metric actor has a nonpositive identity".into());
    }
    for text in [
        Some(&actor.actor.login),
        actor.actor.display_name.as_ref(),
        actor.actor.avatar_url.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        budget_text(used, text, 256 * 1024)?;
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
        coverage.partial("Additional source limitations omitted from this bounded summary; inspect the contributing records");
    }
}

fn valid_node(node: &str) -> bool {
    !node.is_empty() && node.len() <= 1024 && !node.chars().any(char::is_control)
}

fn project_pr(scope: &MetricsScope, pr: &MetricsPrObservation) -> Result<PrEvidence, String> {
    let target = PrTarget {
        repository: pr.repository.full_name.to_ascii_lowercase(),
        number: pr.number,
        repository_id: Some(pr.repository.id),
        database_id: Some(pr.id),
        node_id: Some(pr.node_id.clone()),
        source_record_id: None,
    };
    target.validate(scope)?;
    if pr
        .head_sha
        .as_ref()
        .is_some_and(|head| !crate::search::model::valid_sha(head))
    {
        return Err("Dashboard original head must be a full validated SHA, not a prefix".into());
    }
    if matches!(pr.state, MetricsPrState::Open) && pr.merged == Some(true) {
        return Err("An OPEN inventory contradicts a confirmed merged snapshot".into());
    }
    let state = match &pr.state {
        MetricsPrState::Open => PrState::Open,
        MetricsPrState::Closed if pr.merged == Some(true) => PrState::Merged,
        MetricsPrState::Closed => PrState::ClosedWithoutMergeEvidence,
        MetricsPrState::Unknown(_) => PrState::Unknown,
    };
    Ok(PrEvidence {
        target,
        cohort_member: true,
        title: pr.title.clone(),
        author: MetricActor {
            account_id: pr.author.account_id,
            actor: pr.author.actor.clone(),
        },
        state,
        draft: pr.draft,
        head_sha: pr.head_sha.clone(),
        created_at: pr.created_at.as_ref().map(|date| date.unix_seconds()),
        // Team membership and reusable usernames cannot establish an individual
        // review request; only the actual numeric requested-user source can.
        review_requested_now: pr.requested_users.as_ref().and_then(|users| {
            if users
                .iter()
                .any(|user| user.account_id == Some(scope.account_id()))
            {
                Some(true)
            } else if users.iter().any(|user| user.account_id.is_none()) {
                None
            } else {
                Some(false)
            }
        }),
        timeline: EventCollection::unavailable(),
        reviews: EventCollection::unavailable(),
    })
}

#[cfg(test)]
#[path = "metrics_repository_tests.rs"]
mod tests;
