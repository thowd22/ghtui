use super::model::*;
use std::collections::BTreeMap;

const MAX_RECORDS: usize = 10_000;
const MAX_EVENTS: usize = 50_000;
/// Caller separately authenticates the current scope/read guards before publication or drilldown.
/// This function performs no I/O and never invents missing source evidence.
pub fn dashboard(
    evidence: &DashboardEvidence,
    as_of: u64,
    window: UtcWindow,
) -> Result<DashboardSnapshot, String> {
    evidence.scope.validate()?;
    for (coverage, loaded) in [
        (
            &evidence.pr_coverage,
            evidence.prs.iter().filter(|pr| pr.cohort_member).count(),
        ),
        (&evidence.check_coverage, evidence.checks.len()),
        (&evidence.run_coverage, evidence.runs.len()),
    ] {
        if loaded > 0
            && (coverage.complete_empty || matches!(coverage.state, CoverageState::Unavailable(_)))
        {
            return Err(
                "Unavailable/complete-empty source cannot admit nonempty metric records".into(),
            );
        }
    }
    if window.start >= window.end
        || window.end > as_of
        || evidence.prs.len() > MAX_RECORDS
        || evidence.runs.len() > MAX_RECORDS
        || evidence.checks.len() > MAX_EVENTS
    {
        return Err("Dashboard requires a bounded UTC window ending no later than as-of and bounded source records".into());
    }
    let mut prs = BTreeMap::new();
    let mut event_count = 0usize;
    for pr in &evidence.prs {
        pr.target.validate(&evidence.scope)?;
        event_count = event_count
            .saturating_add(pr.timeline.events.len())
            .saturating_add(pr.reviews.events.len());
        if event_count > MAX_EVENTS {
            return Err("Dashboard retained-event budget exceeded".into());
        }
        let key = (pr.target.repository.to_ascii_lowercase(), pr.target.number);
        if let Some(old) = prs.insert(key, pr) {
            if serde_json::to_value(old).map_err(|error| error.to_string())?
                != serde_json::to_value(pr).map_err(|error| error.to_string())?
            {
                return Err("Conflicting snapshots for one canonical metric PR; refresh instead of double-counting".into());
            }
        }
    }
    let source_cohort = match evidence.cohort {
        PrCohortKind::PersonalQueueUnion => {
            "Personal authored/assigned/review-requested union; not repository-wide"
        }
        PrCohortKind::SelectedRepositoryInventory => "Explicit selected-repository PR inventory",
    };
    let mut cards = vec![];
    for kind in [
        MetricKind::OpenPrs,
        MetricKind::NeedsMyReview,
        MetricKind::OpenAge,
        MetricKind::ReviewWait,
        MetricKind::FirstReview,
        MetricKind::MergeLatency,
    ] {
        let (label,formula)=match kind {
            MetricKind::OpenPrs=>("Loaded open PRs","Count distinct OPEN PRs, including drafts; current loaded snapshot, not a historical trend"),
            MetricKind::NeedsMyReview=>("Needs my review","Count distinct OPEN non-draft PRs with current review-requested membership for the verified account"),
            MetricKind::OpenAge=>("Mean open PR age","Mean(as-of UTC - original created_at) for loaded OPEN PRs including drafts; reopened time does not reset age"),
            MetricKind::ReviewWait=>("Mean requested-review wait","Mean(as-of UTC - latest outstanding request for this numeric account); current OPEN non-draft review queue; complete lifetime timeline required"),
            MetricKind::FirstReview=>("Mean first submitted review latency","Mean(first submitted review UTC - original created_at) for PRs created in [start,end); includes bots/self/draft time and later-dismissed submissions; pending excluded"),
            _=>("Mean merge latency","Mean(verified merged-event UTC - original created_at) for confirmed merged PRs created in [start,end); closed/unmerged and reopened age are not inferred"),
        };
        let mut card = MetricCard {
            kind,
            label: label.into(),
            formula: formula.into(),
            cohort: source_cohort.into(),
            value: MetricValue::Unavailable,
            coverage: evidence.pr_coverage.clone(),
            contributors: vec![],
            unknown: 0,
            censored: 0,
        };
        let historical = matches!(kind, MetricKind::FirstReview | MetricKind::MergeLatency);
        if historical && evidence.cohort == PrCohortKind::PersonalQueueUnion {
            card.coverage.partial("Only loaded personal open-queue PRs are sampled; closed/unrelated PR inventory and historical backfill are missing");
        }
        let mut sum = 0u128;
        let mut samples = 0u64;
        for pr in prs.values() {
            let target = &pr.target;
            if !pr.cohort_member {
                continue;
            }
            if pr.state == PrState::Unknown {
                card.unknown += 1;
                card.contributors.push(unknown_pr(
                    target,
                    pr,
                    "PR state unknown; cohort membership unavailable",
                ));
                continue;
            }
            if matches!(kind, MetricKind::NeedsMyReview | MetricKind::ReviewWait)
                && pr.state == PrState::Open
                && pr.draft != Some(true)
                && (pr.review_requested_now.is_none()
                    || (pr.review_requested_now == Some(true) && pr.draft.is_none()))
            {
                card.unknown += 1;
                card.contributors.push(unknown_pr(
                    target,
                    pr,
                    "Draft/requested-reviewer state missing; non-draft review cohort unavailable",
                ));
                continue;
            }
            let needs = pr.state == PrState::Open
                && pr.draft == Some(false)
                && pr.review_requested_now == Some(true);
            let included = match kind {
                MetricKind::OpenPrs | MetricKind::OpenAge => pr.state == PrState::Open,
                MetricKind::NeedsMyReview | MetricKind::ReviewWait => needs,
                MetricKind::FirstReview | MetricKind::MergeLatency => match pr.created_at {
                    Some(created) if created <= as_of => window.contains(created),
                    _ => {
                        card.unknown += 1;
                        card.contributors.push(unknown_pr(
                            target,
                            pr,
                            "Creation timestamp missing/malformed/future; UTC cohort unknown",
                        ));
                        false
                    }
                },
                _ => false,
            };
            if !included {
                continue;
            }
            let mut contributor = Contributor {
                target: MetricTarget::PullRequest(target.clone()),
                title: pr.title.clone(),
                actors: vec![pr.author.clone()],
                measured_seconds: None,
                health: None,
                evidence: vec![],
                note: String::new(),
            };
            match kind {
                MetricKind::OpenPrs | MetricKind::NeedsMyReview => samples += 1,
                MetricKind::OpenAge => {
                    if let Some(created) = pr.created_at.filter(|created| *created <= as_of) {
                        contributor.measured_seconds = Some(as_of - created)
                    } else {
                        contributor.note="Age unavailable • original creation timestamp is missing/malformed/future".into();
                        card.unknown += 1
                    }
                }
                MetricKind::ReviewWait => {
                    let valid = pr
                        .created_at
                        .filter(|created| *created <= as_of)
                        .is_some_and(|created| {
                            valid_events(
                                &pr.timeline,
                                &evidence.scope,
                                target,
                                created,
                                as_of,
                                false,
                            )
                        });
                    if valid {
                        let latest=pr.timeline.events.iter().filter(|event|matches!(event.kind,EventKind::ReviewRequested{reviewer_id} if reviewer_id==evidence.scope.account_id())).max_by_key(|event|(event.at,event.id));
                        if let Some(request) = latest {
                            let ties=pr.timeline.events.iter().filter(|event|matches!(event.kind,EventKind::ReviewRequested{reviewer_id} if reviewer_id==evidence.scope.account_id()) && event.at==request.at).count();
                            let removed=pr.timeline.events.iter().any(|event|matches!(event.kind,EventKind::ReviewRequestRemoved{reviewer_id} if reviewer_id==evidence.scope.account_id()) && event.at>=request.at);
                            if !removed && ties == 1 {
                                contributor.measured_seconds = Some(as_of - request.at);
                                contributor.evidence.push(MetricTarget::Event {
                                    pr: target.clone(),
                                    event_id: request.id,
                                });
                                contributor.actors.push(request.actor.clone());
                            }
                        }
                    }
                    if contributor.measured_seconds.is_none() {
                        card.unknown += 1;
                        contributor.note="Outstanding request timestamp/complete lifetime timeline unavailable or contradicts current membership".into();
                    }
                }
                MetricKind::FirstReview => {
                    let created = pr
                        .created_at
                        .expect("Validated historical cohort timestamp");
                    if valid_events(&pr.reviews, &evidence.scope, target, created, as_of, true) {
                        if let Some(review) = pr
                            .reviews
                            .events
                            .iter()
                            .min_by_key(|event| (event.at, event.id))
                        {
                            if let EventKind::ReviewSubmitted { review_id, .. } = review.kind {
                                contributor.measured_seconds = Some(review.at - created);
                                contributor.evidence.push(MetricTarget::Review {
                                    pr: target.clone(),
                                    review_id,
                                });
                                contributor.actors.push(review.actor.clone());
                            }
                        } else {
                            card.censored += 1;
                            contributor.note="No submitted review observed in complete lifetime review coverage; censored, not zero latency".into();
                        }
                    } else {
                        card.unknown += 1;
                        contributor.note="Review-category retained events/backfill are incomplete; earliest submitted review is unknown".into();
                    }
                }
                MetricKind::MergeLatency => {
                    if pr.state != PrState::Merged {
                        card.censored += 1;
                        contributor.note="No confirmed merged snapshot; closed/reopened/open is not a merge timestamp".into();
                    } else {
                        let created = pr
                            .created_at
                            .expect("Validated historical cohort timestamp");
                        let merges = pr
                            .timeline
                            .events
                            .iter()
                            .filter(|event| event.kind == EventKind::Merged)
                            .collect::<Vec<_>>();
                        if valid_events(
                            &pr.timeline,
                            &evidence.scope,
                            target,
                            created,
                            as_of,
                            false,
                        ) && merges.len() == 1
                        {
                            let merge = merges[0];
                            contributor.measured_seconds = Some(merge.at - created);
                            contributor.evidence.push(MetricTarget::Event {
                                pr: target.clone(),
                                event_id: merge.id,
                            });
                            contributor.actors.push(merge.actor.clone());
                        } else {
                            card.unknown += 1;
                            contributor.note="Exact merge timestamp/full lifetime timeline missing; updated_at is never substituted".into();
                        }
                    }
                }
                _ => {}
            }
            if let Some(seconds) = contributor.measured_seconds {
                sum += u128::from(seconds);
                samples += 1;
            }
            card.contributors.push(contributor);
        }
        if card.unknown > 0 {
            card.coverage.partial(format!(
                "{} records have unknown state/date/category evidence",
                card.unknown
            ));
        }
        card.value = if matches!(kind, MetricKind::OpenPrs | MetricKind::NeedsMyReview) {
            if samples > 0
                || (matches!(card.coverage.state, CoverageState::Complete)
                    && card.unknown == 0
                    && (prs.values().any(|pr| pr.cohort_member) || card.coverage.complete_empty))
            {
                MetricValue::Count(samples)
            } else {
                MetricValue::Unavailable
            }
        } else if samples > 0 {
            MetricValue::MeanSeconds { sum, samples }
        } else {
            MetricValue::Unavailable
        };
        cards.push(card);
    }
    let mut check_card=health_card(MetricKind::Checks,"Observed exact-head check health","Count observed checks/statuses by outcome for loaded exact PR heads; this is not required-rule completeness",evidence.check_coverage.clone());
    let mut check_seen = BTreeMap::new();
    for check in &evidence.checks {
        check.target.validate(&evidence.scope)?;
        let MetricTarget::Check {
            pr,
            head_sha,
            check_id,
            kind,
        } = &check.target
        else {
            return Err("Check evidence must retain its check destination".into());
        };
        let parent = prs
            .get(&(pr.repository.to_ascii_lowercase(), pr.number))
            .ok_or("Metric check has no current corresponding PR evidence")?;
        if pr != &parent.target {
            return Err(
                "Metric check parent canonical/source identity differs from current PR evidence"
                    .into(),
            );
        }
        if parent.head_sha.as_deref().is_none_or(|expected| {
            !crate::search::model::valid_sha(expected) || !expected.eq_ignore_ascii_case(head_sha)
        }) {
            return Err("Metric check is not bound to the exact full current PR head".into());
        }
        let key = (
            pr.repository.to_ascii_lowercase(),
            pr.number,
            head_sha.to_ascii_lowercase(),
            *check_id,
            kind.clone(),
        );
        if let Some(old) = check_seen.insert(key, check) {
            if old.health != check.health || old.name != check.name {
                return Err("Conflicting outcome for one exact metric check".into());
            }
            continue;
        }
        check_card.contributors.push(Contributor {
            target: check.target.clone(),
            title: check.name.clone(),
            actors: vec![],
            measured_seconds: None,
            health: Some(check.health),
            evidence: vec![],
            note: "Required-rule eligibility is not inferred from observed check health".into(),
        });
    }
    finish_health(&mut check_card);
    cards.push(check_card);
    let mut run_card=health_card(MetricKind::Workflows,"Latest observed workflow-attempt health","One highest observed attempt per scoped run; run created_at in [start,end) UTC, not attempt start/completion or historical success rate",evidence.run_coverage.clone());
    let mut latest: BTreeMap<(String, u64), &RunEvidence> = BTreeMap::new();
    for run in &evidence.runs {
        run.target.validate(&evidence.scope)?;
        let MetricTarget::Run {
            repository,
            run_id,
            attempt,
            workflow_id,
            head_sha,
            ..
        } = &run.target
        else {
            return Err("Run evidence must retain its exact attempt destination".into());
        };
        let key = (repository.to_ascii_lowercase(), *run_id);
        if let Some(old) = latest.get(&key) {
            let MetricTarget::Run {
                repository_id: old_repository,
                attempt: old_attempt,
                workflow_id: old_workflow,
                head_sha: old_head,
                ..
            } = &old.target
            else {
                unreachable!()
            };
            let MetricTarget::Run { repository_id, .. } = &run.target else {
                unreachable!()
            };
            if repository_id != old_repository
                || workflow_id != old_workflow
                || !head_sha.eq_ignore_ascii_case(old_head)
                || run.created_at != old.created_at
            {
                return Err(
                    "One metric run ID has contradictory immutable workflow/head/created identity"
                        .into(),
                );
            }
            if attempt == old_attempt
                && run.observed_at == old.observed_at
                && run.health != old.health
            {
                return Err("Conflicting same-observation run outcome".into());
            }
            if (*attempt, run.observed_at) <= (*old_attempt, old.observed_at) {
                continue;
            }
        }
        latest.insert(key, run);
    }
    for run in latest.values() {
        if run.observed_at > as_of {
            run_card.contributors.push(unknown_run(
                run,
                "Observation timestamp is future relative to dashboard as-of",
            ));
            continue;
        }
        match run.created_at {
            Some(created) if created <= as_of => {
                if !window.contains(created) {
                    continue;
                }
            }
            _ => {
                run_card.contributors.push(unknown_run(
                    run,
                    "Run creation timestamp missing/malformed/future; UTC cohort unknown",
                ));
                continue;
            }
        }
        run_card.contributors.push(Contributor {
            target: run.target.clone(),
            title: run.title.clone(),
            actors: vec![run.actor.clone()],
            measured_seconds: None,
            health: Some(run.health),
            evidence: vec![],
            note: "Latest OBSERVED attempt; updated_at is not a duration/completion timestamp"
                .into(),
        });
    }
    finish_health(&mut run_card);
    cards.push(run_card);
    Ok(DashboardSnapshot{scope:evidence.scope.clone(),as_of,window,cards,notices:vec!["Current loaded snapshots and retained-event samples are not historical trends".into(),"UTC windows are half-open; open-queue snapshot cards include all loaded ages".into(),"Fresh identity, repository access, journal/read guards and coverage must be rechecked before publication and Open".into()]})
}
fn unknown_pr(target: &PrTarget, pr: &PrEvidence, note: &str) -> Contributor {
    Contributor {
        target: MetricTarget::PullRequest(target.clone()),
        title: pr.title.clone(),
        actors: vec![pr.author.clone()],
        measured_seconds: None,
        health: None,
        evidence: vec![],
        note: note.into(),
    }
}
fn unknown_run(run: &RunEvidence, note: &str) -> Contributor {
    Contributor {
        target: run.target.clone(),
        title: run.title.clone(),
        actors: vec![run.actor.clone()],
        measured_seconds: None,
        health: Some(Health::Unknown),
        evidence: vec![],
        note: note.into(),
    }
}
fn valid_events(
    events: &EventCollection,
    scope: &MetricsScope,
    target: &PrTarget,
    created: u64,
    as_of: u64,
    reviews: bool,
) -> bool {
    let Ok(expected_scope) = crate::storage::Scope::github(scope.host(), Some(scope.account_id()))
    else {
        return false;
    };
    if events.binding.as_ref().is_none_or(|binding| {
        binding.scope != expected_scope
            || &binding.parent != target
            || binding.category
                != if reviews {
                    EventCategory::Reviews
                } else {
                    EventCategory::Timeline
                }
    }) {
        return false;
    }
    if !events.covers_lifetime(created, as_of) {
        return false;
    }
    let mut seen = BTreeMap::new();
    for event in &events.events {
        if event.id == 0
            || event.at < created
            || event.at > as_of
            || matches!(event.kind, EventKind::ReviewSubmitted { .. }) != reviews
        {
            return false;
        }
        if matches!(
            event.kind,
            EventKind::ReviewSubmitted { review_id: 0, .. }
                | EventKind::ReviewRequested { reviewer_id: 0 }
                | EventKind::ReviewRequestRemoved { reviewer_id: 0 }
        ) {
            return false;
        }
        if seen.insert(event.id, event).is_some_and(|old| old != event) {
            return false;
        }
    }
    true
}
fn health_card(
    kind: MetricKind,
    label: &str,
    formula: &str,
    coverage: EvidenceCoverage,
) -> MetricCard {
    MetricCard {
        kind,
        label: label.into(),
        formula: formula.into(),
        cohort: "Explicit selected repositories and loaded source records only".into(),
        value: MetricValue::Unavailable,
        coverage,
        contributors: vec![],
        unknown: 0,
        censored: 0,
    }
}
fn finish_health(card: &mut MetricCard) {
    let mut counts = [0u64; 7];
    for contributor in &card.contributors {
        let index = match contributor.health.unwrap_or(Health::Unknown) {
            Health::Success => 0,
            Health::Failure => 1,
            Health::Pending => 2,
            Health::ActionRequired => 3,
            Health::Cancelled => 4,
            Health::Neutral => 5,
            Health::Unknown => 6,
        };
        counts[index] += 1;
    }
    card.unknown += counts[6];
    if card.unknown > 0 {
        card.coverage.partial(format!(
            "{} source outcomes/dates are unknown",
            card.unknown
        ));
    }
    if !card.contributors.is_empty() {
        card.value = MetricValue::HealthCounts {
            success: counts[0],
            failure: counts[1],
            pending: counts[2],
            action_required: counts[3],
            cancelled: counts[4],
            neutral: counts[5],
            unknown: counts[6] + card.unknown.saturating_sub(counts[6]),
        };
    }
}
