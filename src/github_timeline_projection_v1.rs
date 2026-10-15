//! Pure presentation of observed timeline fields, never identity/access proof.
//! Comment/review authors and event generators are distinct documented roles.
use super::{value_actor, PrRow};
use serde_json::Value;
use std::fmt;

#[derive(Debug)]
pub(crate) struct TimelineDisplayLimitV1;
impl fmt::Display for TimelineDisplayLimitV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Timeline display limit reached; original observed section retained")
    }
}
impl std::error::Error for TimelineDisplayLimitV1 {}
pub(crate) fn is_display_limit(error: &anyhow::Error) -> bool {
    error.downcast_ref::<TimelineDisplayLimitV1>().is_some()
}

/// At most two explicit actor-role headers plus original body/label/commit.
/// Conservative even for one-header events; no raw field/row is cropped.
pub(crate) fn row_upper(event: &Value) -> usize {
    event["body"]
        .as_str()
        .map_or(0, |body| body.lines().count())
        .saturating_add(4)
}
pub(crate) fn actor_fields_fit(event: &Value) -> bool {
    ["actor", "user"].iter().all(|key| {
        ["login", "name", "avatar_url", "type"].iter().all(|field| {
            event[*key][*field]
                .as_str()
                .is_none_or(|text| text.len() <= 4096)
        })
    })
}
fn capacity(events: &[Value]) -> Result<usize, TimelineDisplayLimitV1> {
    let (mut rows, mut text) = (0usize, 0usize);
    for event in events {
        if !actor_fields_fit(event) {
            return Err(TimelineDisplayLimitV1);
        }
        let next = row_upper(event);
        rows = rows.checked_add(next).ok_or(TimelineDisplayLimitV1)?;
        let bytes = crate::scoped_pr_observed_tabs_v1::json_upper_bytes(event, 0)
            .ok_or(TimelineDisplayLimitV1)?;
        text = text
            .checked_add(bytes)
            .and_then(|text| text.checked_add(next.checked_mul(128)?))
            .ok_or(TimelineDisplayLimitV1)?;
        if rows > crate::scoped_pr_observed_tabs_v1::MAX_PREPARED_ROWS
            || text > crate::scoped_pr_observed_tabs_v1::MAX_PREPARED_TEXT
        {
            return Err(TimelineDisplayLimitV1);
        }
    }
    Ok(rows)
}
fn scalar<'a>(event: &'a Value, field: &str, fallback: &'a str) -> &'a str {
    event[field]
        .as_str()
        .filter(|text| !text.is_empty())
        .unwrap_or(fallback)
}

#[cfg(test)]
thread_local! { static MATERIALIZED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

pub(super) fn prepare(events: &[Value]) -> Result<Vec<PrRow>, TimelineDisplayLimitV1> {
    // Entire borrowed metadata/body preflight precedes Vec and Actor clones.
    let capacity = capacity(events)?;
    #[cfg(test)]
    MATERIALIZED.with(|count| count.set(count.get() + 1));
    let mut rows = Vec::with_capacity(capacity);
    for event in events {
        let kind = scalar(event, "event", "activity");
        let created = scalar(event, "created_at", "time not provided");
        match kind {
            "commented" => {
                rows.push(PrRow::actor(
                    format!("comment author • commented • {created}"),
                    value_actor(&event["user"]),
                ));
                if event["actor"].is_object() {
                    rows.push(PrRow::actor(
                        format!("event generator • commented • {created}"),
                        value_actor(&event["actor"]),
                    ));
                }
            }
            "reviewed" => {
                let submitted = scalar(event, "submitted_at", "submission time not provided");
                let state = scalar(event, "state", "review state not provided");
                rows.push(PrRow::actor(
                    format!("review author • reviewed • {state} • {submitted}"),
                    value_actor(&event["user"]),
                ));
                if event["actor"].is_object() {
                    // Preserve extra observed metadata without equating it to
                    // the documented reviewer or granting it that role.
                    rows.push(PrRow::actor(
                        format!("reported event actor • reviewed • {submitted}"),
                        value_actor(&event["actor"]),
                    ));
                }
            }
            _ => rows.push(PrRow::actor(
                format!("observed event actor • {kind} • {created}"),
                value_actor(&event["actor"]),
            )),
        }
        if let Some(body) = event["body"].as_str() {
            rows.extend(body.lines().map(PrRow::text));
        }
        if let Some(label) = event["label"]["name"].as_str() {
            rows.push(PrRow::text(format!("Label: {label}")));
        }
        if let Some(sha) = event["commit_id"].as_str() {
            rows.push(PrRow::text(format!("Commit: {sha}")));
        }
    }
    Ok(rows)
}

#[cfg(test)]
#[path = "github_timeline_projection_v1_tests.rs"]
mod tests;
