//! Worker-only metrics retention, independently verified from native Dashboard UI.
//! Current API traversal is NOT retained lifetime/backfill authority. The initial
//! bridge retains facts with HistoryUnavailable; a sealed historical producer is
//! the next slice, not a caller-supplied complete flag or timestamp generation.
use super::*;
use crate::metrics::{
    EventBinding, EventCategory, EventCollection, EventKind, MetricActor, MetricsScope, PrTarget,
    SourceEvent, SubmittedReviewState,
};
use crate::providers::gh::{
    ExactPrMetricEvents, HostIdentity, MetricsPrObservation, MetricsReadCoverage,
    MetricsReadStatus, MetricsRequestedReviewer, MetricsReviewState, MetricsTimelineKind,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod model;
pub use model::*;

const RESERVATION: &str = "metrics-reservation-v1";
const ANCHOR: &str = "metrics-anchor-v1";
const EVENT: &str = "metrics-event-v1";
const STATE: &str = "metrics-state-v1";
const MANIFEST: &str = "metrics-manifest-v1";
const HISTORY_UNAVAILABLE: &str =
    "Current API observations retained; available-history/backfill coverage not yet proven";

fn digest(value: &impl Serialize) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&serde_json::to_value(value)?)?)
    ))
}
fn opaque(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn positive(value: u64) -> bool {
    value > 0 && value <= i64::MAX as u64
}
fn validate_scope(scope: &Scope) -> Result<()> {
    let id = scope
        .login
        .strip_prefix("user-id:")
        .and_then(|id| id.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .context("Metrics retention requires a positive immutable principal")?;
    if Scope::github(scope.host.to_ascii_lowercase(), Some(id))? != *scope
        || scope.host.starts_with('-')
        || scope.host.is_empty()
        || !scope
            .host
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-".contains(&byte))
    {
        bail!("Metrics retention requires a canonical host/user-id scope");
    }
    Ok(())
}
