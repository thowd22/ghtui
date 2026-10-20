//! Retained Dashboard metric history and trend rows (TASK-61 AC2, TASK-62 AC1).
//!
//! Every completed Dashboard load retains one point per card and UTC day for the
//! account, repository scope and window. A point keeps the value, its coverage and
//! the contributing records' canonical IDs and actor IDs, so a trend day can be
//! inspected later. Days without a retained point are labelled missing, never zero,
//! and days before retention began are not backfilled.
use crate::{
    metrics::{
        CoverageState, DashboardSnapshot, Health, MetricCard, MetricKind, MetricTarget, MetricValue,
    },
    metrics_workspace::{utc_label, MetricsPreparedRow, MetricsWorkspaceAction},
    providers::runner::sanitize_terminal,
    storage::{Cache, Scope},
    theme::SemanticToken,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, sync::Arc};

/// Contributors kept per retained point; the rest are counted as omitted.
pub const MAX_RETAINED_CONTRIBUTORS: usize = 200;
const SPARK: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedContributorV1 {
    /// The PR, review, event, check or run label, e.g. `owner/repo#7`.
    pub item: String,
    /// Canonical IDs that identify the record independently of its label.
    pub ids: String,
    /// Each actor as `login (id N)`; `id unavailable` when the source had none.
    pub actors: Vec<String>,
    pub measure: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RetainedCoverageV1 {
    Complete,
    Partial(Vec<String>),
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrendPointV1 {
    pub version: u8,
    pub observed_at: u64,
    /// The plotted number (see [`trend_formula`]); `None` when the value was unavailable.
    pub value: Option<u64>,
    pub value_label: String,
    pub coverage: RetainedCoverageV1,
    pub unknown: u64,
    pub censored: u64,
    pub contributors: Vec<RetainedContributorV1>,
    pub omitted: u64,
}

/// The key that separates trends by repository scope and window length.
pub fn scope_key<'a>(repositories: impl IntoIterator<Item = &'a String>, days: u64) -> String {
    let mut names = repositories
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    format!("{}|{days}d", names.join(","))
}
pub fn card_key(kind: MetricKind) -> String {
    format!("{kind:?}")
}
/// What a trend plots for each card kind.
pub fn trend_formula(kind: MetricKind) -> &'static str {
    match kind {
        MetricKind::OpenPrs | MetricKind::NeedsMyReview => "Trend value: record count",
        MetricKind::OpenAge
        | MetricKind::ReviewWait
        | MetricKind::FirstReview
        | MetricKind::MergeLatency => "Trend value: mean seconds over measured records",
        MetricKind::Checks | MetricKind::Workflows => "Trend value: failed count",
    }
}
pub fn trend_value(value: &MetricValue) -> Option<u64> {
    match value {
        MetricValue::Count(count) => Some(*count),
        MetricValue::MeanSeconds { sum, samples } if *samples > 0 => {
            u64::try_from(sum / u128::from(*samples)).ok()
        }
        MetricValue::HealthCounts { failure, .. } => Some(*failure),
        _ => None,
    }
}
fn window_days(snapshot: &DashboardSnapshot) -> Result<u64, String> {
    let span = snapshot
        .window
        .end
        .checked_sub(snapshot.window.start)
        .ok_or("Dashboard window is invalid")?;
    if span == 0 || span % 86_400 != 0 || span / 86_400 > 90 {
        return Err("Trends need a whole-day UTC window of at most 90 days".into());
    }
    Ok(span / 86_400)
}
fn ids(target: &MetricTarget) -> String {
    let optional =
        |value: Option<u64>| value.map_or_else(|| "unavailable".into(), |id| id.to_string());
    match target {
        MetricTarget::Run {
            repository_id,
            run_id,
            attempt,
            workflow_id,
            head_sha,
            ..
        } => format!(
            "repository id {} • run id {run_id} attempt {attempt} • workflow id {workflow_id} • head {head_sha}",
            optional(*repository_id)
        ),
        MetricTarget::PullRequest(pr)
        | MetricTarget::Review { pr, .. }
        | MetricTarget::Event { pr, .. }
        | MetricTarget::Check { pr, .. } => format!(
            "repository id {} • PR id {} • node {}",
            optional(pr.repository_id),
            optional(pr.database_id),
            pr.node_id.as_deref().unwrap_or("unavailable")
        ),
    }
}
fn health(health: Health) -> &'static str {
    match health {
        Health::Success => "Passed",
        Health::Failure => "Failed",
        Health::Pending => "Pending",
        Health::ActionRequired => "Action required",
        Health::Cancelled => "Cancelled",
        Health::Neutral => "Neutral",
        Health::Unknown => "Unknown",
    }
}
/// One card's retained point, as observed at the snapshot's as-of time.
pub fn point(card: &MetricCard, as_of: u64) -> TrendPointV1 {
    let contributors = card
        .contributors
        .iter()
        .take(MAX_RETAINED_CONTRIBUTORS)
        .map(|contributor| RetainedContributorV1 {
            item: crate::metrics_workspace::target_label(&contributor.target),
            ids: ids(&contributor.target),
            actors: contributor
                .actors
                .iter()
                .map(|actor| {
                    format!(
                        "{} ({})",
                        actor.actor.login,
                        actor
                            .account_id
                            .map_or_else(|| "id unavailable".into(), |id| format!("id {id}"))
                    )
                })
                .collect(),
            measure: match (contributor.health, contributor.measured_seconds) {
                (Some(state), _) => health(state).into(),
                (None, Some(seconds)) => format!("{seconds}s"),
                (None, None) => "current record".into(),
            },
        })
        .collect();
    TrendPointV1 {
        version: 1,
        observed_at: as_of,
        value: trend_value(&card.value),
        value_label: crate::metrics_workspace::value_label(&card.value),
        coverage: match &card.coverage.state {
            CoverageState::Complete => RetainedCoverageV1::Complete,
            CoverageState::Partial(reasons) => RetainedCoverageV1::Partial(reasons.clone()),
            CoverageState::Unavailable(reason) => RetainedCoverageV1::Unavailable(reason.clone()),
        },
        unknown: card.unknown,
        censored: card.censored,
        contributors,
        omitted: card
            .contributors
            .len()
            .saturating_sub(MAX_RETAINED_CONTRIBUTORS) as u64,
    }
}

/// Retained points for one scope: card key → UTC day → point, or why it is unreadable.
pub type RetainedTrends = BTreeMap<String, BTreeMap<u64, Result<TrendPointV1, String>>>;

/// Worker-only: retain today's points for `snapshot` (when `record`), then read the
/// window's retained points back. Never called from drawing.
pub fn retain_and_load(
    database: &Path,
    snapshot: &DashboardSnapshot,
    record: bool,
) -> Result<RetainedTrends, String> {
    let days = window_days(snapshot)?;
    let scope = Scope::github(snapshot.scope.host(), Some(snapshot.scope.account_id()))
        .map_err(|error| error.to_string())?;
    let key = scope_key(snapshot.scope.repositories(), days);
    let last_day = snapshot.as_of / 86_400;
    let cache = Cache::open(database).map_err(|error| error.to_string())?;
    if record {
        let points = snapshot
            .cards
            .iter()
            .map(|card| {
                serde_json::to_value(point(card, snapshot.as_of))
                    .map(|value| (card_key(card.kind), value))
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        cache
            .retain_metric_trend_points_v1(&scope, &key, last_day, &points)
            .map_err(|error| error.to_string())?;
    }
    let mut output = RetainedTrends::new();
    for (card, day, value) in cache
        .metric_trend_points_v1(
            &scope,
            &key,
            last_day + 1 - days.min(last_day + 1),
            last_day,
        )
        .map_err(|error| error.to_string())?
    {
        let point = serde_json::from_value::<TrendPointV1>(value)
            .map_err(|error| format!("unreadable retained point: {error}"))
            .and_then(|point| {
                (point.version == 1 && point.observed_at / 86_400 == day)
                    .then_some(point)
                    .ok_or_else(|| "retained point version or day differs".to_string())
            });
        output.entry(card).or_default().insert(day, point);
    }
    Ok(output)
}

/// A card's prepared trend: its list rows and, per retained day, that day's records.
#[derive(Clone, Debug)]
pub struct PreparedTrend {
    pub rows: Arc<[MetricsPreparedRow]>,
    pub days: Arc<[Arc<[MetricsPreparedRow]>]>,
}
fn prose(label: String, role: SemanticToken) -> MetricsPreparedRow {
    MetricsPreparedRow {
        label: sanitize_terminal(&label),
        role,
        actor: None,
        avatar_only: false,
        action: None,
        cells: Vec::new(),
    }
}
pub fn day_label(day: u64) -> String {
    utc_label(day.saturating_mul(86_400))
        .chars()
        .take(10)
        .collect()
}
fn coverage_text(coverage: &RetainedCoverageV1) -> String {
    match coverage {
        RetainedCoverageV1::Complete => "Complete".into(),
        RetainedCoverageV1::Partial(reasons) => format!("Partial: {}", reasons.join("; ")),
        RetainedCoverageV1::Unavailable(reason) => format!("Unavailable: {reason}"),
    }
}
/// The sparkline for consecutive days: `·` missing, `?` unavailable or unreadable.
pub fn sparkline(values: &[Option<Result<Option<u64>, ()>>]) -> String {
    let max = values
        .iter()
        .filter_map(|value| match value {
            Some(Ok(Some(value))) => Some(*value),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    values
        .iter()
        .map(|value| match value {
            None => '·',
            Some(Ok(Some(_))) if max == 0 => SPARK[0],
            Some(Ok(Some(value))) => {
                SPARK[((u128::from(*value) * 7 + u128::from(max) / 2) / u128::from(max)) as usize]
            }
            Some(_) => '?',
        })
        .collect()
}
/// Prepares one card's trend rows over the snapshot window's UTC days.
pub fn prepare(
    kind: MetricKind,
    snapshot: &DashboardSnapshot,
    retained: &Result<RetainedTrends, String>,
) -> PreparedTrend {
    let empty = BTreeMap::new();
    let (points, failure) = match retained {
        Ok(trends) => (trends.get(&card_key(kind)).unwrap_or(&empty), None),
        Err(reason) => (&empty, Some(reason.clone())),
    };
    let days = window_days(snapshot).unwrap_or(0);
    let last_day = snapshot.as_of / 86_400;
    let first_day = last_day + 1 - days.min(last_day + 1);
    let mut rows = vec![prose(trend_formula(kind).into(), SemanticToken::Muted)];
    if let Some(reason) = failure {
        rows.push(prose(
            format!("Trend history unavailable (not zero): {reason}"),
            SemanticToken::Pending,
        ));
        return PreparedTrend {
            rows: Arc::from(rows),
            days: Arc::from([]),
        };
    }
    let retained = points.values().filter(|point| point.is_ok()).count();
    let partial = points
        .values()
        .filter(|point| {
            matches!(
                point,
                Ok(TrendPointV1 {
                    coverage: RetainedCoverageV1::Partial(_) | RetainedCoverageV1::Unavailable(_),
                    ..
                })
            )
        })
        .count();
    let first = points
        .keys()
        .next()
        .map_or_else(|| "none".into(), |day| day_label(*day));
    rows.push(prose(
        format!(
            "Retained {retained} of {days} UTC days ({partial} partial) • first retained {first} • days before retention began are not backfilled"
        ),
        if retained as u64 == days && partial == 0 {
            SemanticToken::Success
        } else {
            SemanticToken::Pending
        },
    ));
    let values = (first_day..=last_day)
        .map(|day| {
            points.get(&day).map(|point| match point {
                Ok(point) => Ok(point.value),
                Err(_) => Err(()),
            })
        })
        .collect::<Vec<_>>();
    rows.push(prose(
        format!(
            "Trend {} • oldest to newest • · missing day (not zero) • ? unavailable",
            sparkline(&values)
        ),
        SemanticToken::Normal,
    ));
    let mut detail = Vec::new();
    for day in first_day..=last_day {
        let date = day_label(day);
        match points.get(&day) {
            None => rows.push(prose(
                format!("{date}  Missing: no retained observation (not zero)"),
                SemanticToken::Muted,
            )),
            Some(Err(reason)) => rows.push(prose(
                format!("{date}  Unreadable retained point (not zero): {reason}"),
                SemanticToken::Pending,
            )),
            Some(Ok(point)) => {
                let complete = point.coverage == RetainedCoverageV1::Complete;
                let mut row = prose(
                    format!(
                        "{date}  {}  • {} • {} records",
                        point.value_label,
                        coverage_text(&point.coverage),
                        point.contributors.len() as u64 + point.omitted
                    ),
                    if complete {
                        SemanticToken::Normal
                    } else {
                        SemanticToken::Pending
                    },
                );
                row.action = Some(MetricsWorkspaceAction::TrendDay(detail.len()));
                rows.push(row);
                let mut lines = vec![
                    prose(
                        format!(
                            "{date} retained observation • observed {}",
                            utc_label(point.observed_at)
                        ),
                        SemanticToken::Head,
                    ),
                    prose(
                        format!(
                            "Value {} • Unknown {} • Censored {}",
                            point.value_label, point.unknown, point.censored
                        ),
                        SemanticToken::Normal,
                    ),
                    prose(
                        coverage_text(&point.coverage),
                        if complete {
                            SemanticToken::Success
                        } else {
                            SemanticToken::Pending
                        },
                    ),
                    prose(
                        format!(
                            "Contributing records retained: {}{}",
                            point.contributors.len(),
                            if point.omitted > 0 {
                                format!(" • {} more not retained", point.omitted)
                            } else {
                                String::new()
                            }
                        ),
                        SemanticToken::Muted,
                    ),
                ];
                for contributor in &point.contributors {
                    lines.push(prose(
                        format!("{} • {}", contributor.item, contributor.measure),
                        SemanticToken::Reference,
                    ));
                    lines.push(prose(
                        format!("  IDs: {}", contributor.ids),
                        SemanticToken::Muted,
                    ));
                    lines.push(prose(
                        format!(
                            "  Actors: {}",
                            if contributor.actors.is_empty() {
                                "none recorded".into()
                            } else {
                                contributor.actors.join(", ")
                            }
                        ),
                        SemanticToken::Actor,
                    ));
                }
                detail.push(Arc::from(lines));
            }
        }
    }
    PreparedTrend {
        rows: Arc::from(rows),
        days: Arc::from(detail),
    }
}
