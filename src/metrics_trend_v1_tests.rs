//! Retained metric history and trend widgets (TASK-61 AC2, TASK-62 AC1).
use super::*;
use crate::metrics_trend_v1::{
    retain_and_load, scope_key, RetainedContributorV1, RetainedCoverageV1, TrendPointV1,
};
use crate::storage::{Cache, Scope};
use std::os::unix::fs::PermissionsExt;

fn private_temp() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn key() -> String {
    scope_key(&["example/b".to_string(), "example/a".to_string()], 7)
}
fn seeded(day: u64) -> TrendPointV1 {
    TrendPointV1 {
        version: 1,
        observed_at: day * 86_400 + 3_600,
        value: Some(4),
        value_label: "4".into(),
        coverage: RetainedCoverageV1::Partial(vec!["seeded tools inventory missing".into()]),
        unknown: 1,
        censored: 0,
        contributors: vec![RetainedContributorV1 {
            item: "example/a#9".into(),
            ids: "repository id 9 • PR id 109 • node seeded-node".into(),
            actors: vec!["bob (id 7)".into()],
            measure: "current record".into(),
        }],
        omitted: 0,
    }
}
fn seed(database: &std::path::Path, account: u64, key: &str, day: u64) {
    let cache = Cache::open(database).unwrap();
    cache
        .retain_metric_trend_points_v1(
            &Scope::github("github.invalid", Some(account)).unwrap(),
            key,
            day,
            &[("OpenPrs".into(), serde_json::to_value(seeded(day)).unwrap())],
        )
        .unwrap();
}
fn open_prs(workspace: &MetricsWorkspace) -> usize {
    workspace
        .snapshot
        .cards
        .iter()
        .position(|card| card.kind == MetricKind::OpenPrs)
        .unwrap()
}
fn labels(workspace: &MetricsWorkspace) -> Vec<String> {
    workspace
        .rows()
        .iter()
        .map(|row| row.label.clone())
        .collect()
}

#[test]
fn metric_trends_retain_ids_actors_and_label_missing_partial_and_unbackfilled_days() {
    let temp = private_temp();
    let database = temp.path().join("cache.sqlite3");
    let mut workspace = prepared();
    let today = workspace.snapshot.as_of / 86_400;
    seed(&database, 42, &key(), today - 2);
    // Another account and another scope never leak into this trend.
    seed(&database, 43, &key(), today - 1);
    seed(
        &database,
        42,
        &scope_key(&["example/a".to_string()], 7),
        today - 3,
    );
    let retained = retain_and_load(&database, &workspace.snapshot, true).unwrap();
    let points = &retained["OpenPrs"];
    assert_eq!(
        points.keys().copied().collect::<Vec<_>>(),
        [today - 2, today]
    );
    let now = points[&today].as_ref().unwrap();
    assert_eq!(now.observed_at, workspace.snapshot.as_of);
    assert_eq!(now.value, Some(1));
    let contributor = &now.contributors[0];
    assert_eq!(contributor.item, "example/a#7");
    assert_eq!(
        contributor.ids,
        "repository id 9 • PR id 107 • node genuine-pr-node"
    );
    assert_eq!(contributor.actors, ["octocat (id 42)"]);
    assert_eq!(retained.len(), 8, "every card retains a point");

    workspace.attach_trends(Ok(retained));
    workspace.apply(MetricsWorkspaceAction::Trend(0)).unwrap();
    assert_eq!(workspace.view, MetricsView::Trend(open_prs(&workspace)));
    let rows = labels(&workspace);
    assert!(rows[0].contains("record count"));
    assert!(rows[1].starts_with("Retained 2 of 7 UTC days (1 partial)"));
    assert!(rows[1].contains("not backfilled"));
    assert!(rows[2].starts_with("Trend ····█·▃ "), "{}", rows[2]);
    let missing = crate::metrics_trend_v1::day_label(today - 1);
    assert!(rows
        .iter()
        .any(|row| row == &format!("{missing}  Missing: no retained observation (not zero)")));
    let partial = crate::metrics_trend_v1::day_label(today - 2);
    let position = rows
        .iter()
        .position(|row| row.starts_with(&partial))
        .unwrap();
    assert!(rows[position].contains("Partial: seeded tools inventory missing"));
    assert_eq!(rows.len(), 3 + 7, "one row per UTC day in the window");
    // Missing days are not actions; retained days are.
    assert!(workspace.rows()[position - 1].action.is_none());
    workspace
        .apply(MetricsWorkspaceAction::Select(position))
        .unwrap();
    let action = workspace.selected_action().unwrap();
    assert_eq!(action, MetricsWorkspaceAction::TrendDay(0));
    workspace.apply(action).unwrap();
    let detail = labels(&workspace);
    assert!(detail
        .iter()
        .any(|row| row == "example/a#9 • current record"));
    assert!(detail
        .iter()
        .any(|row| row == "  IDs: repository id 9 • PR id 109 • node seeded-node"));
    assert!(detail.iter().any(|row| row == "  Actors: bob (id 7)"));
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert_eq!(workspace.view, MetricsView::Trend(open_prs(&workspace)));
    assert_eq!(workspace.selected, position);
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert_eq!(workspace.view, MetricsView::Cards);
    assert_eq!(workspace.selected, 0);
}

#[test]
fn metric_trends_same_day_upserts_and_prunes_beyond_retention() {
    let temp = private_temp();
    let database = temp.path().join("cache.sqlite3");
    let workspace = prepared();
    let today = workspace.snapshot.as_of / 86_400;
    seed(&database, 42, &key(), today - 100);
    retain_and_load(&database, &workspace.snapshot, true).unwrap();
    retain_and_load(&database, &workspace.snapshot, true).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let cache = Cache::open(&database).unwrap();
    let all = cache
        .metric_trend_points_v1(&scope, &key(), 0, today)
        .unwrap();
    assert_eq!(all.len(), 8, "one point per card and day; old days pruned");
    assert!(all.iter().all(|(_, day, _)| *day == today));
    // Read-only loading (a cancelled request) never writes.
    let other = Scope::github("github.invalid", Some(44)).unwrap();
    assert!(cache
        .metric_trend_points_v1(&other, &key(), 0, today)
        .unwrap()
        .is_empty());
}

#[test]
fn metric_trends_unavailable_unreadable_and_unprepared_history_is_never_zero() {
    let mut workspace = prepared();
    assert!(workspace
        .apply(MetricsWorkspaceAction::Trend(0))
        .unwrap_err()
        .contains("Refresh first"));
    workspace.attach_trends(Err("database is locked".into()));
    workspace.apply(MetricsWorkspaceAction::Trend(0)).unwrap();
    assert!(labels(&workspace)
        .iter()
        .any(|row| row == "Trend history unavailable (not zero): database is locked"));
    assert!(workspace
        .apply(MetricsWorkspaceAction::TrendDay(0))
        .is_err());
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    workspace.apply(MetricsWorkspaceAction::Layout).unwrap();
    assert!(workspace.apply(MetricsWorkspaceAction::Trend(0)).is_err());

    let temp = private_temp();
    let database = temp.path().join("cache.sqlite3");
    let today = workspace.snapshot.as_of / 86_400;
    let mut wrong = seeded(today - 1);
    wrong.observed_at = (today - 5) * 86_400;
    Cache::open(&database)
        .unwrap()
        .retain_metric_trend_points_v1(
            &Scope::github("github.invalid", Some(42)).unwrap(),
            &key(),
            today - 1,
            &[("OpenPrs".into(), serde_json::to_value(wrong).unwrap())],
        )
        .unwrap();
    let retained = retain_and_load(&database, &workspace.snapshot, false).unwrap();
    assert!(retained["OpenPrs"][&(today - 1)].is_err());
    assert_eq!(retained.len(), 1, "a read-only load retains nothing new");
    let mut fresh = prepared();
    fresh.attach_trends(Ok(retained));
    fresh.apply(MetricsWorkspaceAction::Trend(0)).unwrap();
    let rows = labels(&fresh);
    assert!(rows[1].starts_with("Retained 0 of 7 UTC days"));
    assert!(rows
        .iter()
        .any(|row| row.contains("Unreadable retained point (not zero)")));
}

#[test]
fn metric_trends_sparkline_scales_and_marks_missing_and_unavailable() {
    use crate::metrics_trend_v1::sparkline;
    assert_eq!(
        sparkline(&[
            None,
            Some(Ok(Some(0))),
            Some(Ok(Some(8))),
            Some(Ok(None)),
            Some(Err(()))
        ]),
        "·▁█??"
    );
    assert_eq!(sparkline(&[Some(Ok(Some(0))), None]), "▁·");
}

#[test]
fn metric_trends_footer_key_click_and_enter_emit_the_same_actions() {
    let mut workspace = prepared();
    workspace.attach_trends(Ok(Default::default()));
    workspace.apply(MetricsWorkspaceAction::Select(1)).unwrap();
    let controls = metrics_ui::footer_keys(&workspace);
    let trend = controls.iter().find(|control| control.key == "t").unwrap();
    assert_eq!(trend.label, "Trend");
    assert!(trend.disabled.is_none());
    assert_eq!(
        trend.action,
        crate::app::Action::Metrics(MetricsWorkspaceAction::Trend(1))
    );
    workspace.apply(MetricsWorkspaceAction::Trend(1)).unwrap();
    let card = workspace
        .snapshot
        .cards
        .iter()
        .position(|card| card.kind == MetricKind::NeedsMyReview)
        .unwrap();
    assert_eq!(workspace.view, MetricsView::Trend(card));
    let (buffer, rendered) = render_v91(&mut workspace, 120, 30);
    let screen = lines_v91(&buffer).join("\n");
    assert!(screen.contains("Needs my review"));
    assert!(screen.contains("Retained 0 of 7 UTC days"));
    assert!(screen.contains("Missing: no retained observation (not zero)"));
    assert!(!rendered.hits.is_empty());
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    workspace.apply(MetricsWorkspaceAction::Layout).unwrap();
    let controls = metrics_ui::footer_keys(&workspace);
    let trend = controls.iter().find(|control| control.key == "t").unwrap();
    assert!(trend
        .disabled
        .as_deref()
        .unwrap()
        .contains("Return to dashboard cards"));
}
