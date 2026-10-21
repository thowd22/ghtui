//! TASK-62 Dashboard unresolved-review-threads widget. The worker reads the
//! current review threads of the loaded open PRs on explicit request only (never
//! in the shared Dashboard load); this module chooses those PRs and projects the
//! reads into rows whose Enter/click opens the exact PR. Drawing reads rows only.
use crate::{
    metrics_workspace::{MetricsPreparedRow, MetricsWorkspace},
    providers::{
        gh::{
            MetricsPrObservation, MetricsPrState, MetricsReviewThreadsSnapshot,
            MetricsThreadReplyTo,
        },
        runner::sanitize_terminal,
    },
    theme::SemanticToken,
};

/// At most this many open PRs are read per request; the rest are named as not read.
pub const THREAD_PR_LIMIT: usize = 10;

/// Loaded open PRs with a full head, in repository/number order, and how many
/// further open PRs exceed [`THREAD_PR_LIMIT`].
pub fn candidates(observations: &[MetricsPrObservation]) -> (Vec<&MetricsPrObservation>, usize) {
    let mut open = observations
        .iter()
        .filter(|pr| {
            matches!(pr.state, MetricsPrState::Open)
                && pr
                    .head_sha
                    .as_ref()
                    .is_some_and(|head| crate::search::model::valid_sha(head))
        })
        .collect::<Vec<_>>();
    open.sort_by(|left, right| {
        left.repository
            .full_name
            .cmp(&right.repository.full_name)
            .then_with(|| left.number.cmp(&right.number))
    });
    let skipped = open.len().saturating_sub(THREAD_PR_LIMIT);
    open.truncate(THREAD_PR_LIMIT);
    (open, skipped)
}

/// One PR's read: its current threads, or why they are unavailable.
pub type ThreadRead<'a> = (
    &'a MetricsPrObservation,
    Result<MetricsReviewThreadsSnapshot, String>,
);

/// Rows (one per unresolved thread, PR order then path) and the widget summary,
/// which names every partial source instead of counting it as zero.
pub fn project(
    workspace: &MetricsWorkspace,
    reads: &[ThreadRead<'_>],
    skipped: usize,
) -> (Vec<MetricsPreparedRow>, String) {
    let host = workspace.snapshot.scope.host().to_owned();
    let mut rows = Vec::new();
    let mut partial = Vec::new();
    let mut outdated_total = 0usize;
    let mut prs_with = 0usize;
    let mut unknown = 0usize;
    for (pr, read) in reads {
        let item = format!("{}#{}", pr.repository.full_name, pr.number);
        let snapshot = match read {
            Ok(snapshot) => snapshot,
            Err(error) => {
                partial.push(format!("{item} unavailable: {error}"));
                continue;
            }
        };
        if !snapshot.state_coverage.enumeration_complete
            || !snapshot.state_coverage.metric_evidence_complete
        {
            partial.push(format!("{item} thread list incomplete"));
        }
        let mut threads = snapshot
            .threads
            .iter()
            .filter(|thread| {
                if thread.resolved.is_none() {
                    unknown += 1;
                }
                thread.resolved == Some(false)
            })
            .collect::<Vec<_>>();
        threads.sort_by(|left, right| {
            left.placement
                .observed_path
                .cmp(&right.placement.observed_path)
                .then_with(|| left.placement.line.cmp(&right.placement.line))
                .then_with(|| left.node_id.cmp(&right.node_id))
        });
        if !threads.is_empty() {
            prs_with += 1;
        }
        for thread in threads {
            let place = match (
                &thread.placement.observed_path,
                thread.placement.line.or(thread.placement.original_line),
            ) {
                (Some(path), Some(line)) => format!("{path}:{line}"),
                (Some(path), None) => format!("{path} (file)"),
                (None, _) => "(path unavailable)".into(),
            };
            let (measure, role) = match thread.outdated {
                Some(true) => {
                    outdated_total += 1;
                    ("outdated", SemanticToken::Muted)
                }
                Some(false) => ("current", SemanticToken::Pending),
                None => ("state unknown", SemanticToken::Muted),
            };
            let author = thread
                .comments
                .iter()
                .find(|comment| comment.reply_to == MetricsThreadReplyTo::Original)
                .or_else(|| thread.comments.first())
                .and_then(|comment| comment.author.as_ref());
            let login = author.map_or("unknown author", |actor| actor.actor.login.as_str());
            rows.push(MetricsPreparedRow {
                label: sanitize_terminal(&format!(
                    "{login} • {item} • {place} • {measure} • {} comments",
                    thread.comments.len()
                )),
                role,
                actor: author
                    .map(|actor| crate::identity::ActorIdentity::github(&host, &actor.actor)),
                avatar_only: true,
                action: Some(workspace.open_pr_action(pr)),
                cells: vec![
                    crate::ui_table::Cell::styled(
                        sanitize_terminal(&item),
                        crate::theme::Role::TextMuted,
                    ),
                    sanitize_terminal(&place).into(),
                    String::new().into(),
                    crate::ui_table::Cell::styled(measure, role.role()),
                ],
            });
        }
    }
    if unknown > 0 {
        partial.push(format!("{unknown} threads with unknown resolved state"));
    }
    if skipped > 0 {
        partial.push(format!(
            "{skipped} open PRs beyond the {THREAD_PR_LIMIT}-PR read limit not read"
        ));
    }
    let summary = if reads.is_empty() && skipped == 0 {
        "No loaded open PRs • no thread read sent".to_owned()
    } else {
        format!(
            "{} unresolved review threads on {prs_with} of {} open PRs ({outdated_total} outdated) • {}",
            rows.len(),
            reads.len() + skipped,
            if partial.is_empty() {
                "Complete".to_owned()
            } else {
                format!("Partial: {}", partial.join("; "))
            }
        )
    };
    (rows, summary)
}

#[cfg(test)]
#[path = "dashboard_threads_v1_tests.rs"]
mod tests;
