//! TASK-62 stale-PR and unresolved-thread widget contracts (code level; the PTY
//! journeys live in tests/tui_e2e/dashboard_widgets_v1.rs).
use super::*;
use crate::{
    metrics::{
        dashboard, CoverageState, DashboardEvidence, EvidenceCoverage, MetricTarget, MetricsAction,
        PrCohortKind, UtcWindow,
    },
    metrics_workspace::{MetricsView, MetricsWorkspaceAction, STALE_DAYS},
    providers::gh::{
        Actor, HostIdentity, MetricsActor, MetricsReadCoverage, MetricsReadStatus,
        MetricsRepository, MetricsReviewThreadObservation, MetricsThreadComment,
        MetricsThreadPlacement, MetricsThreadProvenanceCoverage, MetricsTimestamp,
    },
};
use std::sync::Arc;

const AS_OF: u64 = 1_800_000_000;

fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "fixture".into(),
    }
}
fn actor(login: &str) -> MetricsActor {
    MetricsActor {
        account_id: Some(7),
        actor: Actor {
            login: login.into(),
            actor_type: "User".into(),
            ..Actor::default()
        },
    }
}
fn repository() -> MetricsRepository {
    MetricsRepository {
        host: "github.invalid".into(),
        id: 9,
        node_id: "genuine-repo-node".into(),
        full_name: "example/a".into(),
    }
}
fn stamp(seconds: u64) -> Option<MetricsTimestamp> {
    MetricsTimestamp::parse(&format!(
        "{}Z",
        crate::metrics_workspace::utc_label(seconds)
            .trim_end_matches(" UTC")
            .replace(' ', "T")
    ))
}
fn pr(number: u64, open: bool, draft: bool, idle_days: Option<u64>) -> MetricsPrObservation {
    MetricsPrObservation {
        repository: repository(),
        id: 100 + number,
        node_id: format!("PR_node_{number}"),
        number,
        title: format!("Change {number}"),
        author: actor(&format!("author{number}")),
        state: if open {
            MetricsPrState::Open
        } else {
            MetricsPrState::Closed
        },
        draft: Some(draft),
        created_at: None,
        merged_at: None,
        closed_at: None,
        merged: Some(false),
        head_sha: Some("a".repeat(40)),
        requested_users: Some(vec![]),
        requested_teams: Some(vec![]),
        observed_updated_at: idle_days.and_then(|days| stamp(AS_OF - days * 86_400)),
        reasons: vec![],
    }
}
fn workspace(observations: Vec<MetricsPrObservation>) -> MetricsWorkspace {
    let scope = crate::metrics::MetricsScope::new(&identity(), &["example/a".into()]).unwrap();
    let evidence = Arc::new(DashboardEvidence {
        scope,
        cohort: PrCohortKind::SelectedRepositoryInventory,
        prs: vec![],
        pr_coverage: EvidenceCoverage {
            state: CoverageState::Complete,
            complete_empty: false,
            fetched_at: Some(AS_OF),
        },
        checks: vec![],
        check_coverage: EvidenceCoverage::unavailable("Checks were not requested"),
        runs: vec![],
        run_coverage: EvidenceCoverage::unavailable("Actions were not requested"),
    });
    let snapshot = Arc::new(
        dashboard(
            &evidence,
            AS_OF,
            UtcWindow::new(AS_OF - 7 * 86_400, AS_OF).unwrap(),
        )
        .unwrap(),
    );
    MetricsWorkspace::prepare(
        evidence,
        snapshot,
        vec!["example/a".into()],
        Arc::from(observations),
        Arc::from(vec![repository()]),
    )
    .unwrap()
}
fn open_of(workspace: &MetricsWorkspace, number: u64) -> Option<MetricsWorkspaceAction> {
    let observation = workspace
        .observations
        .iter()
        .find(|pr| pr.number == number)
        .unwrap();
    Some(MetricsWorkspaceAction::Metric(MetricsAction::Open(
        MetricsWorkspace::pr_target(observation),
    )))
}

#[test]
fn dashboard_widgets_stale_prs_are_open_idle_oldest_first_and_open_their_exact_pr() {
    assert!(stamp(AS_OF).is_some_and(|value| value.unix_seconds() == AS_OF));
    let mut workspace = workspace(vec![
        pr(1, true, false, Some(10)),
        pr(2, true, false, Some(1)),
        pr(3, true, true, Some(20)),
        pr(4, false, false, Some(30)),
        pr(5, true, false, None),
        pr(6, true, false, Some(STALE_DAYS)),
    ]);
    let items = workspace
        .stale_rows
        .iter()
        .map(|row| row.cells[0].text.clone())
        .collect::<Vec<_>>();
    assert_eq!(items, ["example/a#3", "example/a#1", "example/a#6"]);
    assert_eq!(workspace.stale_rows[0].cells[1].text, "Draft: Change 3");
    assert_eq!(workspace.stale_rows[0].cells[3].text, "idle 20d");
    assert_eq!(
        workspace.stale_rows[0].actor.as_ref().unwrap().login,
        Some("author3".to_owned())
    );
    assert!(workspace
        .stale_summary
        .starts_with("3 open PRs not updated for 7+ days"));
    assert!(workspace
        .stale_summary
        .ends_with("1 without updated_at (not counted)"));
    assert_eq!(workspace.stale_rows[1].action, open_of(&workspace, 1));

    // Key and click share one action; Back returns to the card left.
    workspace.apply(MetricsWorkspaceAction::Select(2)).unwrap();
    workspace.apply(MetricsWorkspaceAction::Stale).unwrap();
    assert_eq!(workspace.view, MetricsView::Stale);
    assert_eq!(workspace.selected, 0);
    workspace.apply(MetricsWorkspaceAction::Select(1)).unwrap();
    let request = workspace
        .apply(workspace.selected_action().unwrap())
        .unwrap();
    assert!(matches!(
        request,
        Some(MetricsAction::Open(MetricTarget::PullRequest(ref target))) if target.number == 1
            && target.database_id == Some(101)
            && target.node_id.as_deref() == Some("PR_node_1")
    ));
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert_eq!(
        (workspace.view, workspace.selected),
        (MetricsView::Cards, 2)
    );
    assert!(workspace
        .apply(MetricsWorkspaceAction::Layout)
        .and_then(|_| workspace.apply(MetricsWorkspaceAction::Stale))
        .is_err());
}

#[test]
fn dashboard_widgets_thread_candidates_are_bounded_open_full_head_prs_in_order() {
    let mut observations = (1..=12)
        .rev()
        .map(|number| pr(number, true, false, Some(1)))
        .collect::<Vec<_>>();
    observations.push(pr(20, false, false, Some(1)));
    let mut headless = pr(21, true, false, Some(1));
    headless.head_sha = None;
    observations.push(headless);
    let (chosen, skipped) = candidates(&observations);
    assert_eq!(
        chosen.iter().map(|pr| pr.number).collect::<Vec<_>>(),
        (1..=10).collect::<Vec<_>>()
    );
    assert_eq!(skipped, 2);
    assert_eq!(candidates(&[]).0.len(), 0);
}

fn coverage(complete: bool) -> MetricsReadCoverage {
    MetricsReadCoverage {
        enumeration_complete: complete,
        metric_evidence_complete: true,
        complete_empty: false,
        requested_pages: 1,
        fetched_pages: 1,
        fetched_rows: 3,
        admitted_rows: 3,
        excluded_rows: 0,
        response_bytes: 10,
        reasons: vec![],
        status: if complete {
            MetricsReadStatus::Complete
        } else {
            MetricsReadStatus::Partial
        },
    }
}
fn comment(id: &str, root: bool, login: &str) -> MetricsThreadComment {
    MetricsThreadComment {
        node_id: id.into(),
        database_id: None,
        reply_to: if root {
            MetricsThreadReplyTo::Original
        } else {
            MetricsThreadReplyTo::Reply("C_root".into())
        },
        author: Some(actor(login)),
        body: Some("private body is never shown in the widget".into()),
        created_at: None,
        updated_at: None,
        observed_path: None,
        line: None,
        start_line: None,
        original_line: None,
        original_start_line: None,
        commit_sha: None,
        original_commit_sha: None,
        reasons: vec![],
    }
}
fn thread(
    id: &str,
    resolved: Option<bool>,
    outdated: Option<bool>,
    path: &str,
    line: Option<u64>,
) -> MetricsReviewThreadObservation {
    MetricsReviewThreadObservation {
        parent_node_id: "PR_node_1".into(),
        node_id: id.into(),
        subject_type: None,
        resolved,
        outdated,
        viewer_can_reply: None,
        viewer_can_resolve: None,
        viewer_can_unresolve: None,
        resolved_by: None,
        placement: MetricsThreadPlacement {
            observed_path: Some(path.into()),
            observed_side: None,
            observed_start_side: None,
            line,
            start_line: None,
            original_line: Some(3),
            original_start_line: None,
        },
        // The reply is listed first; the original comment's author still leads.
        comments: vec![
            comment("C_reply", false, "bob"),
            comment("C_root", true, "alice"),
        ],
        comment_coverage: coverage(true),
        original_comment: None,
        state_reasons: vec![],
        provenance_reasons: vec![],
    }
}
fn snapshot(
    parent: &MetricsPrObservation,
    threads: Vec<MetricsReviewThreadObservation>,
    complete: bool,
) -> MetricsReviewThreadsSnapshot {
    MetricsReviewThreadsSnapshot {
        identity: identity(),
        parent: parent.clone(),
        threads,
        state_coverage: coverage(complete),
        provenance_coverage: MetricsThreadProvenanceCoverage {
            comment_enumeration_complete: true,
            actors_complete: true,
            content_complete: true,
            original_candidates_complete: true,
            original_tree_verification_available: false,
            reasons: vec![],
        },
        observed_from: AS_OF,
        observed_through: AS_OF,
        requests: 7,
        graphql_pages: 1,
        raw_comments: 2,
        aggregate_response_bytes: 10,
        successful_diagnostic_bytes: 0,
        failed_reserved_bytes: 0,
    }
}

#[test]
fn dashboard_widgets_unresolved_threads_count_only_unresolved_and_name_partial_sources() {
    let mut workspace = workspace(vec![
        pr(1, true, false, Some(1)),
        pr(2, true, false, Some(1)),
        pr(3, true, false, Some(1)),
    ]);
    let observations = workspace.observations.clone();
    let reads: Vec<ThreadRead<'_>> = vec![
        (
            &observations[0],
            Ok(snapshot(
                &observations[0],
                vec![
                    thread("T_z", Some(false), Some(false), "src/z.rs", Some(9)),
                    thread("T_resolved", Some(true), Some(false), "src/a.rs", Some(1)),
                    thread("T_a", Some(false), Some(true), "src/a.rs", None),
                    thread("T_unknown", None, None, "src/b.rs", Some(2)),
                ],
                true,
            )),
        ),
        (
            &observations[1],
            Ok(snapshot(&observations[1], vec![], false)),
        ),
        (&observations[2], Err("rate limited".into())),
    ];
    let (rows, summary) = project(&workspace, &reads, 4);
    assert_eq!(
        rows.iter()
            .map(|row| (row.cells[1].text.clone(), row.cells[3].text.clone()))
            .collect::<Vec<_>>(),
        [
            ("src/a.rs:3".to_owned(), "outdated".to_owned()),
            ("src/z.rs:9".to_owned(), "current".to_owned()),
        ]
    );
    assert!(rows.iter().all(|row| row.cells[0].text == "example/a#1"
        && row.actor.as_ref().unwrap().login.as_deref() == Some("alice")
        && row.action == open_of(&workspace, 1)
        && !row.label.contains("private body")));
    assert_eq!(
        summary,
        "2 unresolved review threads on 1 of 7 open PRs (1 outdated) • Partial: example/a#2 thread list incomplete; example/a#3 unavailable: rate limited; 1 threads with unknown resolved state; 4 open PRs beyond the 10-PR read limit not read"
    );
    let (_, empty) = project(&workspace, &[], 0);
    assert_eq!(empty, "No loaded open PRs • no thread read sent");

    // The worker request hides old rows; its result opens the widget and Back returns.
    workspace.apply(MetricsWorkspaceAction::Select(1)).unwrap();
    assert_eq!(
        workspace
            .apply(MetricsWorkspaceAction::Metric(MetricsAction::Threads))
            .unwrap(),
        Some(MetricsAction::Threads)
    );
    assert!(workspace.pending.is_some() && workspace.rows().is_empty());
    workspace.pending = None;
    workspace.attach_threads(rows, summary);
    assert_eq!(workspace.view, MetricsView::Threads);
    assert_eq!(workspace.rows().len(), 2);
    assert!(workspace.apply(MetricsWorkspaceAction::Inspect(0)).is_err());
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert_eq!(
        (workspace.view, workspace.selected),
        (MetricsView::Cards, 1)
    );
}

#[test]
fn dashboard_widgets_footer_keys_have_lowest_priority_and_layout_disables_them() {
    let mut workspace = workspace(vec![pr(1, true, false, Some(10))]);
    let keys = crate::metrics_ui::footer_keys(&workspace);
    let stale = keys.iter().find(|key| key.key == "z").unwrap();
    let threads = keys.iter().find(|key| key.key == "u").unwrap();
    assert_eq!(
        stale.action,
        crate::app::Action::Metrics(MetricsWorkspaceAction::Stale)
    );
    assert_eq!(
        threads.action,
        crate::app::Action::Metrics(MetricsWorkspaceAction::Metric(MetricsAction::Threads))
    );
    let lowest = keys
        .iter()
        // N Dashboards (named dashboards) is the only control below these two.
        .filter(|key| !["z", "u", "N"].contains(&key.key.as_str()))
        .map(|key| key.priority)
        .max()
        .unwrap();
    assert!(stale.priority > lowest && threads.priority > stale.priority);
    assert!(stale.disabled.is_none() && threads.disabled.is_none());
    workspace.apply(MetricsWorkspaceAction::Layout).unwrap();
    let keys = crate::metrics_ui::footer_keys(&workspace);
    assert!(keys
        .iter()
        .filter(|key| key.key == "z" || key.key == "u")
        .all(|key| key.disabled.is_some()));
    let app = crate::app::App {
        view: crate::app::View::Dashboard,
        metrics: Some(workspace),
        ..Default::default()
    };
    assert!(crate::navigation_ui::assert_help_and_palette_cover_footer_v97(&app, "Dashboard") > 5);
}
