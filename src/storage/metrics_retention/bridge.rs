use super::*;

fn coverage(value: &MetricsReadCoverage) -> ObservationCoverage {
    let mut reasons = value
        .reasons
        .iter()
        .take(63)
        .map(|reason| {
            if reason.len() <= 1024 {
                reason.clone()
            } else {
                "Oversized provider reason omitted; source remains incomplete".into()
            }
        })
        .collect::<Vec<_>>();
    let reason_overflow =
        value.reasons.len() > 63 || value.reasons.iter().any(|reason| reason.len() > 1024);
    if reason_overflow {
        reasons
            .push("Provider reasons exceeded retention budget; source remains incomplete".into());
    }
    ObservationCoverage {
        enumeration_complete: value.enumeration_complete,
        semantic_complete: value.metric_evidence_complete && !reason_overflow,
        fetched_pages: value.fetched_pages,
        fetched_rows: value.fetched_rows,
        excluded_rows: value.excluded_rows,
        response_bytes: value.response_bytes,
        unavailable: matches!(value.status, MetricsReadStatus::Unavailable(_)),
        reasons,
    }
}
fn metric_actor(value: &crate::providers::gh::MetricsActor) -> MetricActor {
    MetricActor {
        account_id: value.account_id,
        actor: value.actor.clone(),
    }
}
fn submitted(value: &MetricsReviewState) -> Option<SubmittedReviewState> {
    match value {
        MetricsReviewState::Approved => Some(SubmittedReviewState::Approved),
        MetricsReviewState::ChangesRequested => Some(SubmittedReviewState::ChangesRequested),
        MetricsReviewState::Commented => Some(SubmittedReviewState::Commented),
        MetricsReviewState::Dismissed => Some(SubmittedReviewState::Dismissed),
        MetricsReviewState::Pending | MetricsReviewState::Unknown(_) => None,
    }
}
impl MetricCategoryBatch {
    pub fn from_provider(value: &ExactPrMetricEvents, category: EventCategory) -> Result<Self> {
        if value.observed_from > value.observed_through {
            bail!("Provider metric observation interval is reversed");
        }
        let target = MetricEventTarget::from_provider(&value.identity, &value.parent, category)?;
        let source = target.source()?;
        let source_coverage = match category {
            EventCategory::Reviews => &value.review_coverage,
            EventCategory::Timeline => &value.timeline_coverage,
        };
        let source_rows = match category {
            EventCategory::Reviews => value.reviews.len(),
            EventCategory::Timeline => value.timeline.len(),
        };
        if source_coverage.admitted_rows != source_rows
            || source_coverage.fetched_rows
                < source_coverage
                    .admitted_rows
                    .saturating_add(source_coverage.excluded_rows)
            || source_coverage.fetched_pages > source_coverage.requested_pages
            || source_coverage.requested_pages > 100
            || source_coverage.enumeration_complete && source_coverage.fetched_pages == 0
            || source_coverage.metric_evidence_complete
                && (!source_coverage.enumeration_complete
                    || source_coverage.excluded_rows != 0
                    || !matches!(source_coverage.status, MetricsReadStatus::Complete))
            || source_coverage.complete_empty
                && (!source_coverage.enumeration_complete || source_coverage.fetched_rows != 0)
        {
            bail!("Provider metric rows/status/page coverage contradict the typed collection");
        }
        let mut coverage = coverage(source_coverage);
        let mut facts = vec![];
        match category {
            EventCategory::Reviews => {
                for review in &value.reviews {
                    if matches!(review.state, MetricsReviewState::Unknown(_))
                        || matches!(review.state, MetricsReviewState::Pending)
                            && review.submitted_at.is_some()
                    {
                        coverage.semantic_complete = false;
                    }
                    let Some(state) = submitted(&review.state) else {
                        continue; // Pending cannot become a submitted metric fact.
                    };
                    let Some(at) = &review.submitted_at else {
                        coverage.semantic_complete = false;
                        continue;
                    };
                    facts.push(Fact {
                        source: source.clone(),
                        provider_node: review.node_id.clone(),
                        event: SourceEvent {
                            id: review.id,
                            at: at.unix_seconds(),
                            kind: EventKind::ReviewSubmitted {
                                review_id: review.id,
                                state,
                            },
                            actor: metric_actor(&review.actor),
                        },
                    });
                }
            }
            EventCategory::Timeline => {
                for event in &value.timeline {
                    let kind = match &event.kind {
                        MetricsTimelineKind::ReviewRequested {
                            reviewer: MetricsRequestedReviewer::User(actor),
                        } => actor
                            .account_id
                            .map(|reviewer_id| EventKind::ReviewRequested { reviewer_id }),
                        MetricsTimelineKind::ReviewRequestRemoved {
                            reviewer: MetricsRequestedReviewer::User(actor),
                        } => actor
                            .account_id
                            .map(|reviewer_id| EventKind::ReviewRequestRemoved { reviewer_id }),
                        MetricsTimelineKind::Merged => Some(EventKind::Merged),
                        MetricsTimelineKind::Closed => Some(EventKind::Closed),
                        MetricsTimelineKind::Reopened => Some(EventKind::Reopened),
                        MetricsTimelineKind::ReadyForReview => Some(EventKind::ReadyForReview),
                        MetricsTimelineKind::ConvertedToDraft => Some(EventKind::ConvertedToDraft),
                        MetricsTimelineKind::Unrelated(_)
                        | MetricsTimelineKind::ReviewDismissed { .. } => continue,
                        // Submission evidence has a separately enumerated Reviews source.
                        MetricsTimelineKind::ReviewSubmitted { .. } => continue,
                        _ => {
                            coverage.semantic_complete = false;
                            None
                        }
                    };
                    let (Some(kind), Some(id), Some(node), Some(at)) =
                        (kind, event.id, &event.node_id, &event.at)
                    else {
                        coverage.semantic_complete = false;
                        continue;
                    };
                    facts.push(Fact {
                        source: source.clone(),
                        provider_node: node.clone(),
                        event: SourceEvent {
                            id,
                            at: at.unix_seconds(),
                            kind,
                            actor: metric_actor(&event.actor),
                        },
                    });
                }
            }
        }
        if !coverage.semantic_complete {
            coverage
                .reasons
                .push("Incomplete/unsupported metric observations cannot advance history".into());
        }
        coverage.reasons.sort();
        coverage.reasons.dedup();
        Ok(Self {
            target,
            facts,
            coverage,
            observed_from: value.observed_from,
            observed_through: value.observed_through,
        })
    }
}
