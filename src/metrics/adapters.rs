//! Thin current-model projection. No source fetch, historical completeness or numeric-ID guess.
use super::model::*;
use crate::{
    actions_workspace::ActionsWorkspace,
    github_workspace::{Freshness, GithubWorkspace},
    providers::gh::{Actor, Coverage, PullRequest, Queue},
};
use serde_json::Value;

pub fn current_evidence(
    scope: MetricsScope,
    github: Option<&GithubWorkspace>,
    actions: Option<&ActionsWorkspace>,
) -> Result<DashboardEvidence, String> {
    let mut result = DashboardEvidence {
        scope,
        cohort: PrCohortKind::PersonalQueueUnion,
        prs: vec![],
        pr_coverage: EvidenceCoverage::unavailable("No loaded personal queue union"),
        checks: vec![],
        check_coverage: EvidenceCoverage::unavailable("No exact-head PR check source loaded"),
        runs: vec![],
        run_coverage: EvidenceCoverage::unavailable("No Actions run catalogue loaded"),
    };
    if let Some(github) = github {
        if github.inbox.pull_requests.len() > 10_000 {
            return Err("Dashboard PR projection exceeds10,000-record budget".into());
        }
        if !result.scope.matches(&github.identity) || !result.scope.matches(&github.inbox.identity)
        {
            return Err(
                "Dashboard PR source belongs to a different account/host/current queue login"
                    .into(),
            );
        }
        result.prs = github
            .inbox
            .pull_requests
            .iter()
            .filter(|pr| {
                result.scope.accepts(&pr.repository) && !github.exclusions.contains(&pr.repository)
            })
            .map(|pr| pr_evidence(&result.scope, pr, true))
            .collect::<Result<Vec<_>, _>>()?;
        let fetched = match &github.freshness {
            Freshness::Empty => None,
            Freshness::Cached { fetched_at }
            | Freshness::Fresh { fetched_at }
            | Freshness::Offline { fetched_at, .. } => Some(*fetched_at),
        };
        result.pr_coverage = coverage(&github.inbox.coverage, result.prs.is_empty(), fetched);
        match &github.freshness {
            Freshness::Fresh { .. } => {}
            Freshness::Cached { .. } => result
                .pr_coverage
                .partial("Cached queue snapshot; current remote truth is not established"),
            Freshness::Offline { .. } => result
                .pr_coverage
                .partial("Offline/stale queue snapshot; counts describe observed records"),
            Freshness::Empty => {
                if !result.prs.is_empty() {
                    return Err("Unloaded PR source cannot admit queued private records".into());
                }
                result.pr_coverage = EvidenceCoverage::unavailable("No personal queues loaded");
            }
        }
        if let Some(page) = github
            .pr
            .as_ref()
            .filter(|page| result.scope.accepts(&page.details.summary.pr.repository))
        {
            let mut detail = pr_evidence(&result.scope, &page.details.summary.pr, false)?;
            detail.state = if page.details.summary.merged {
                PrState::Merged
            } else {
                detail.state
            };
            detail.head_sha = Some(page.details.summary.head_sha.clone());
            detail.timeline = observed_timeline(&page.details.timeline);
            detail.reviews = observed_reviews(&page.details.reviews);
            if let Some(existing) = result.prs.iter_mut().find(|pr| {
                pr.target
                    .repository
                    .eq_ignore_ascii_case(&detail.target.repository)
                    && pr.target.number == detail.target.number
            }) {
                detail.cohort_member = true;
                detail.review_requested_now = existing.review_requested_now;
                *existing = detail;
            } else if result.scope.accepts(&detail.target.repository) {
                result.prs.push(detail);
            }
            if let Some(checks) = &page.checks {
                result.check_coverage = EvidenceCoverage {
                    state: CoverageState::Complete,
                    complete_empty: checks.is_empty(),
                    fetched_at: fetched,
                };
                for check in checks {
                    if check
                        .head_sha
                        .eq_ignore_ascii_case(&page.details.summary.head_sha)
                    {
                        let target = MetricTarget::Check {
                            pr: PrTarget {
                                repository: page.details.summary.pr.repository.to_ascii_lowercase(),
                                number: page.details.summary.pr.number,
                                repository_id: None,
                                database_id: None,
                                node_id: None,
                                source_record_id: Some(page.details.summary.pr.id.clone()),
                            },
                            head_sha: check.head_sha.to_ascii_lowercase(),
                            check_id: check.id,
                            kind: check.kind.clone(),
                        };
                        target.validate(&result.scope)?;
                        result.checks.push(CheckEvidence {
                            target,
                            name: check.name.clone(),
                            health: health(&check.status, &check.conclusion),
                        });
                    } else {
                        result.check_coverage.partial("Mismatched/incomplete check head excluded; never attach a similarly prefixed SHA");
                    }
                }
                if result.prs.iter().filter(|pr| pr.cohort_member).count() > 1 {
                    result.check_coverage.partial(
                        "Checks are loaded for one selected PR detail only, not every queued PR",
                    );
                }
            }
        }
    }
    if let Some(actions) = actions {
        if actions.runs.len() > 10_000 {
            return Err("Dashboard run projection exceeds10,000-record budget".into());
        }
        if !result.scope.matches(&actions.identity) {
            return Err("Dashboard Actions source belongs to a different account/host".into());
        }
        result.runs = actions
            .runs
            .iter()
            .filter(|run| result.scope.accepts(&run.repository))
            .map(|source| {
                let run = &source.run;
                let target = MetricTarget::Run {
                    repository: source.repository.to_ascii_lowercase(),
                    repository_id: None,
                    run_id: run.id,
                    attempt: run.attempt,
                    workflow_id: run.workflow_id,
                    head_sha: run.head_sha.to_ascii_lowercase(),
                };
                target.validate(&result.scope)?;
                Ok(RunEvidence {
                    target,
                    title: run.title.clone(),
                    actor: metric_actor(&run.triggering_actor),
                    created_at: utc_seconds(&run.created_at),
                    observed_at: actions
                        .fetched_at
                        .ok_or("Actions source has no observed fetch time")?,
                    health: health(&run.status, &run.conclusion),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        result.run_coverage = coverage(
            &actions.coverage,
            result.runs.is_empty(),
            actions.fetched_at,
        );
        let filter = &actions.filter;
        if filter.workflow_id.is_some()
            || filter.actor.is_some()
            || filter.branch.is_some()
            || filter.event.is_some()
            || filter.status.is_some()
            || filter.head_sha.is_some()
            || filter.created.is_some()
        {
            result.run_coverage.partial("Existing Actions filters define an observed subset, not full repository/window coverage");
        }
        if actions
            .repositories
            .iter()
            .any(|repository| !result.scope.accepts(repository))
            || result.scope.repositories().iter().any(|repository| {
                !actions
                    .repositories
                    .iter()
                    .any(|loaded| loaded.eq_ignore_ascii_case(repository))
            })
        {
            result
                .run_coverage
                .partial("Actions loaded repository set differs from explicit dashboard scope");
        }
        if actions.fetched_at.is_none() {
            result.run_coverage =
                EvidenceCoverage::unavailable("No Actions catalogue observation loaded");
        }
    }
    Ok(result)
}
fn coverage(source: &Coverage, empty: bool, fetched_at: Option<u64>) -> EvidenceCoverage {
    EvidenceCoverage {
        state: if source.complete {
            CoverageState::Complete
        } else {
            CoverageState::Partial(if source.reasons.is_empty() {
                vec!["Source enumeration is partial or unavailable".into()]
            } else {
                source.reasons.clone()
            })
        },
        complete_empty: source.complete && empty,
        fetched_at,
    }
}
fn metric_actor(actor: &Actor) -> MetricActor {
    MetricActor {
        account_id: None,
        actor: actor.clone(),
    }
}
fn value_actor(value: &Value) -> MetricActor {
    MetricActor {
        account_id: value["id"].as_u64().filter(|id| *id > 0),
        actor: Actor {
            login: value["login"].as_str().unwrap_or_default().into(),
            display_name: value["name"].as_str().map(str::to_owned),
            avatar_url: value["avatar_url"].as_str().map(str::to_owned),
            actor_type: value["type"].as_str().unwrap_or("Unknown").into(),
        },
    }
}
fn pr_evidence(
    scope: &MetricsScope,
    pr: &PullRequest,
    cohort_member: bool,
) -> Result<PrEvidence, String> {
    if !pr.host.eq_ignore_ascii_case(scope.host()) {
        return Err("Metric PR host differs from verified scope".into());
    }
    let target = PrTarget {
        repository: pr.repository.to_ascii_lowercase(),
        number: pr.number,
        repository_id: None,
        database_id: None,
        node_id: None,
        source_record_id: (!pr.id.is_empty()).then(|| pr.id.clone()),
    };
    target.validate(scope)?;
    Ok(PrEvidence {
        target,
        cohort_member,
        title: pr.title.clone(),
        author: metric_actor(&pr.author),
        state: match pr.state.to_ascii_lowercase().as_str() {
            "open" => PrState::Open,
            "merged" => PrState::Merged,
            "closed" => PrState::ClosedWithoutMergeEvidence,
            _ => PrState::Unknown,
        },
        draft: Some(pr.draft),
        head_sha: None,
        created_at: utc_seconds(&pr.created_at),
        review_requested_now: Some(pr.queues.contains(&Queue::ReviewRequested)),
        timeline: EventCollection::unavailable(),
        reviews: EventCollection::unavailable(),
    })
}
/// Projection preserves observed metadata only. It cannot attest first/last historical events.
pub fn observed_reviews(values: &[Value]) -> EventCollection {
    let mut collection = EventCollection::unavailable();
    if values.len() > 50_000 {
        collection.reasons.push(
            "Review projection exceeds50,000-event budget; no prefix is historical authority"
                .into(),
        );
        return collection;
    }
    collection
        .reasons
        .push("Current PrDetails file coverage is not retained review/backfill coverage".into());
    for value in values {
        let state = match value["state"].as_str() {
            Some("APPROVED") => SubmittedReviewState::Approved,
            Some("CHANGES_REQUESTED") => SubmittedReviewState::ChangesRequested,
            Some("COMMENTED") => SubmittedReviewState::Commented,
            Some("DISMISSED") => SubmittedReviewState::Dismissed,
            Some("PENDING") => continue,
            _ => {
                collection.reasons.push("Unknown review state".into());
                continue;
            }
        };
        if let (Some(id), Some(at)) = (
            value["id"].as_u64().filter(|id| *id > 0),
            value["submitted_at"].as_str().and_then(utc_seconds),
        ) {
            collection.events.push(SourceEvent {
                id,
                at,
                kind: EventKind::ReviewSubmitted {
                    review_id: id,
                    state,
                },
                actor: value_actor(&value["user"]),
            });
        } else {
            collection
                .reasons
                .push("Submitted review omitted its actual ID/time".into());
        }
    }
    collection
}
pub fn observed_timeline(values: &[Value]) -> EventCollection {
    let mut collection = EventCollection::unavailable();
    if values.len() > 50_000 {
        collection.reasons.push(
            "Timeline projection exceeds50,000-event budget; no prefix is historical authority"
                .into(),
        );
        return collection;
    }
    collection.reasons.push(
        "Current timeline metadata is not committed retained/backfill lifetime coverage".into(),
    );
    for value in values {
        let kind = match value["event"].as_str() {
            Some("merged") => EventKind::Merged,
            Some("reopened") => EventKind::Reopened,
            Some("closed") => EventKind::Closed,
            Some("ready_for_review") => EventKind::ReadyForReview,
            Some("converted_to_draft") => EventKind::ConvertedToDraft,
            Some("review_requested" | "review_request_removed") => {
                let Some(reviewer_id) = value["requested_reviewer"]["id"]
                    .as_u64()
                    .filter(|id| *id > 0)
                else {
                    collection
                        .reasons
                        .push("User request target ID omitted/team membership not verified".into());
                    continue;
                };
                if value["event"] == "review_requested" {
                    EventKind::ReviewRequested { reviewer_id }
                } else {
                    EventKind::ReviewRequestRemoved { reviewer_id }
                }
            }
            _ => continue,
        };
        if let (Some(id), Some(at)) = (
            value["id"].as_u64().filter(|id| *id > 0),
            value["created_at"].as_str().and_then(utc_seconds),
        ) {
            collection.events.push(SourceEvent {
                id,
                at,
                kind,
                actor: value_actor(&value["actor"]),
            });
        } else {
            collection
                .reasons
                .push("Relevant timeline event omitted actual ID/time".into());
        }
    }
    collection
}
