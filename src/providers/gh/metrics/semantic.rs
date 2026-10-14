//! Source-only formula relevance and REST source-URL helpers. Declare as a
//! child of parse.rs only with the reviewed semantic activation hunk.
//! Documented schema: https://docs.github.com/en/rest/using-the-rest-api/issue-event-types
use super::*;

pub(super) const REVIEW_COMMIT_UNAVAILABLE: &str =
    "Review source commit is malformed; no current-head substitution";
pub(super) const REVIEW_TARGET_UNAVAILABLE: &str =
    "Review target is missing or a team; viewer membership is not inferred";

pub(super) fn documented_unrelated(name: &str) -> bool {
    matches!(
        name,
        "assigned"
            | "unassigned"
            | "labeled"
            | "unlabeled"
            | "milestoned"
            | "demilestoned"
            | "commented"
            | "referenced"
            | "cross-referenced"
            | "committed"
            | "head_ref_force_pushed"
            | "head_ref_deleted"
            | "head_ref_restored"
            | "base_ref_changed"
            | "base_ref_force_pushed"
            | "mentioned"
            | "subscribed"
            | "unsubscribed"
            | "automatic_base_change_failed"
            | "automatic_base_change_succeeded"
            | "connected"
            | "disconnected"
            | "deployed"
            | "deployment_environment_changed"
            | "locked"
            | "unlocked"
            | "marked_as_duplicate"
            | "unmarked_as_duplicate"
            | "pinned"
            | "unpinned"
            | "renamed"
            | "user_blocked"
    )
}

pub(super) fn timeline_source_url(
    parent: &MetricsPrObservation,
    value: &Value,
    name: &str,
    id: Option<u64>,
) -> GithubResult<()> {
    if name == "committed" {
        let Some(url) = value.get("url").filter(|url| !url.is_null()) else {
            return Ok(());
        };
        let sha = value["sha"]
            .as_str()
            .filter(|sha| crate::search::model::valid_sha(sha))
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Timeline commit URL lacks an actual full source SHA; no anchor invented",
                )
            })?;
        let root = format!("repos/{}", parent.repository.full_name);
        if !url.as_str().is_some_and(|url| {
            api_path(
                &parent.repository.host,
                url,
                &format!("{root}/commits/{sha}"),
                true,
            ) || api_path(
                &parent.repository.host,
                url,
                &format!("{root}/git/commits/{sha}"),
                true,
            )
        }) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Timeline commit URL contradicts exact host/repository/source SHA",
            ));
        }
        return Ok(());
    }
    // No event ID is inferred from a URL. Missing irrelevant identity stays
    // nullable; relevant identity is separately required by the parser.
    let Some(id) = id else { return Ok(()) };
    let path = match name {
        "commented" => format!("repos/{}/issues/comments/{id}", parent.repository.full_name),
        "reviewed" => format!(
            "repos/{}/pulls/{}/reviews/{id}",
            parent.repository.full_name, parent.number
        ),
        _ => format!("repos/{}/issues/events/{id}", parent.repository.full_name),
    };
    optional_url(value, "url", &parent.repository.host, &path, true)
}

pub(crate) fn review_metric_reasons(review: &MetricsReviewObservation) -> Vec<String> {
    // Commit provenance is still nullable and its reason still lives on the
    // source observation. First-submission latency does not use a commit anchor.
    review
        .reasons
        .iter()
        .filter(|reason| reason.as_str() != REVIEW_COMMIT_UNAVAILABLE)
        .cloned()
        .collect()
}

pub(crate) fn timeline_metric_reasons(event: &MetricsTimelineObservation) -> Vec<String> {
    if matches!(
        event.kind,
        MetricsTimelineKind::Unrelated(_)
            | MetricsTimelineKind::ReviewSubmitted { .. }
            | MetricsTimelineKind::ReviewDismissed { .. }
    ) {
        // Reviews are independently exhausted, timed and retained. Redundant
        // timeline presentation/provenance cannot erase known merge/request
        // evidence. Raw nullable fields/reasons remain on the observation.
        vec![]
    } else {
        // Known team/unknown target remains explicit on the typed event. It
        // cannot prove a direct viewer request, but is not an unknown merge.
        event
            .reasons
            .iter()
            .filter(|reason| reason.as_str() != REVIEW_TARGET_UNAVAILABLE)
            .cloned()
            .collect()
    }
}

/// Namespace comes from the closed observed REST resource kind, never from a
/// positive number or an opaque-node prefix. These REST tables have separate
/// numeric identities; nodes remain globally checked by the page collector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TimelineNumericNamespace {
    IssueEvent,
    IssueComment,
    PullRequestReview,
}
pub(crate) fn timeline_numeric_key(
    event: &MetricsTimelineObservation,
) -> Option<(TimelineNumericNamespace, u64)> {
    use TimelineNumericNamespace::*;
    let id = event.id?;
    let namespace = match &event.kind {
        MetricsTimelineKind::ReviewSubmitted { .. } => PullRequestReview,
        MetricsTimelineKind::Unrelated(name) if name == "commented" => IssueComment,
        // These distinct documented shapes have no numeric ID field. A
        // supplied extra id cannot invent an Event/Commit/Reference identity.
        MetricsTimelineKind::Unrelated(name)
            if matches!(name.as_str(), "committed" | "cross-referenced") =>
        {
            return None
        }
        MetricsTimelineKind::Unsupported(_) => return None,
        _ => IssueEvent,
    };
    Some((namespace, id))
}

/// The page collector needs the same typed numeric namespaces as its final
/// contradiction check, including when an unrelated record has no node ID.
/// Globally supplied opaque nodes still share one exact keyspace. Unknown or
/// documented no-numeric-resource shapes remain distinct observations, never
/// identities synthesized from an extra numeric field.
pub(crate) fn timeline_collection_key(
    event: &MetricsTimelineObservation,
    unidentified: &mut usize,
) -> String {
    if let Some(node) = &event.node_id {
        return format!("node:{node}");
    }
    if let Some((namespace, id)) = timeline_numeric_key(event) {
        return format!("numeric:{namespace:?}:{id}");
    }
    *unidentified += 1;
    format!("unidentified-observation:{unidentified}")
}
