use super::*;
use crate::providers::gh::{Actor, MetricsActor, MetricsReadCoverage, MetricsRepository};

fn identity(id: u64) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "viewer".into(),
        account_id: Some(id),
        gh_version: "2.99.0".into(),
    }
}
fn actor(id: Option<u64>, login: &str) -> MetricsActor {
    MetricsActor {
        account_id: id,
        actor: Actor {
            login: login.into(),
            display_name: None,
            avatar_url: None,
            actor_type: "User".into(),
        },
    }
}
fn parent() -> MetricsPrObservation {
    MetricsPrObservation {
        repository: MetricsRepository {
            host: "github.invalid".into(),
            id: 50,
            node_id: "R_Genuine".into(),
            full_name: "org/app".into(),
        },
        id: 700,
        node_id: "PR_Genuine".into(),
        number: 7,
        title: "Canonical dashboard parent".into(),
        author: actor(Some(10), "author"),
        state: MetricsPrState::Open,
        draft: Some(false),
        created_at: None,
        merged_at: None,
        closed_at: None,
        merged: Some(false),
        head_sha: Some("a".repeat(40)),
        requested_users: Some(vec![actor(Some(42), "renamed-viewer")]),
        requested_teams: Some(vec![]),
        observed_updated_at: None,
        reasons: vec![],
    }
}
fn inventory(prs: Vec<MetricsPrObservation>) -> RepositoryPrInventory {
    RepositoryPrInventory {
        identity: identity(42),
        repository: parent().repository,
        coverage: MetricsReadCoverage {
            enumeration_complete: true,
            metric_evidence_complete: true,
            complete_empty: prs.is_empty(),
            requested_pages: 1,
            fetched_pages: 1,
            fetched_rows: prs.len(),
            admitted_rows: prs.len(),
            excluded_rows: 0,
            response_bytes: 500,
            reasons: vec![],
            status: MetricsReadStatus::Complete,
        },
        prs,
        observed_from: 100,
        observed_through: 101,
        requests: 3,
        aggregate_response_bytes: 500,
    }
}
fn project(sources: &[RepositoryPrInventory]) -> Result<DashboardEvidence, String> {
    repository_evidence(&identity(42), &["org/app".into()], sources)
}

#[test]
fn dashboard_repository_canonical_inventory_retains_numeric_actor_parent_and_no_fake_history() {
    let evidence = project(&[inventory(vec![parent()])]).unwrap();
    assert_eq!(evidence.cohort, PrCohortKind::SelectedRepositoryInventory);
    let pr = &evidence.prs[0];
    assert!(pr.target.has_canonical_identity());
    assert_eq!(pr.target.repository_id, Some(50));
    assert_eq!(pr.target.database_id, Some(700));
    assert_eq!(pr.target.node_id.as_deref(), Some("PR_Genuine"));
    assert_eq!(pr.target.source_record_id, None);
    assert_eq!(pr.author.account_id, Some(10));
    assert_eq!(pr.review_requested_now, Some(true));
    assert!(pr.created_at.is_none());
    assert!(!pr.timeline.covers_lifetime(1, 101) && !pr.reviews.covers_lifetime(1, 101));
    assert!(matches!(
        evidence.check_coverage.state,
        CoverageState::Unavailable(_)
    ));
    assert!(matches!(
        evidence.run_coverage.state,
        CoverageState::Unavailable(_)
    ));
}

#[test]
fn dashboard_repository_missing_scope_is_not_complete_empty_or_zero() {
    let empty = project(&[inventory(vec![])]).unwrap();
    assert_eq!(empty.pr_coverage.state, CoverageState::Complete);
    assert!(empty.pr_coverage.complete_empty);
    let missing = project(&[]).unwrap();
    assert!(matches!(
        missing.pr_coverage.state,
        CoverageState::Unavailable(_)
    ));
    assert!(!missing.pr_coverage.complete_empty);
    let partial = repository_evidence(
        &identity(42),
        &["org/app".into(), "org/other".into()],
        &[inventory(vec![])],
    )
    .unwrap();
    assert!(matches!(
        partial.pr_coverage.state,
        CoverageState::Partial(_)
    ));
    assert!(!partial.pr_coverage.complete_empty);
}

#[test]
fn dashboard_repository_username_team_unknown_id_never_invents_viewer_request() {
    for (users, expected) in [
        (None, None),
        (Some(vec![actor(None, "viewer")]), None),
        (Some(vec![actor(Some(43), "viewer")]), Some(false)),
        (Some(vec![]), Some(false)),
        (
            Some(vec![actor(None, "unknown"), actor(Some(42), "renamed")]),
            Some(true),
        ),
    ] {
        let mut pr = parent();
        pr.requested_users = users;
        assert_eq!(
            project(&[inventory(vec![pr])]).unwrap().prs[0].review_requested_now,
            expected
        );
    }
}

#[test]
fn dashboard_repository_foreign_account_repository_or_parent_identity_refuses() {
    let mut wrong = inventory(vec![parent()]);
    wrong.identity = identity(43);
    assert!(project(&[wrong]).is_err());
    let mut wrong = inventory(vec![parent()]);
    wrong.repository.id = 51;
    assert!(project(&[wrong]).is_err());
    let mut wrong = inventory(vec![parent()]);
    wrong.prs[0].node_id.clear();
    assert!(project(&[wrong]).is_err());
    let mut wrong = inventory(vec![parent()]);
    wrong.prs[0].id = 0;
    assert!(project(&[wrong]).is_err());
    let mut wrong = inventory(vec![parent()]);
    wrong.prs[0].head_sha = Some("abcdef".into());
    assert!(project(&[wrong]).is_err());
    let mut wrong = parent();
    wrong.merged = Some(true);
    assert!(project(&[inventory(vec![wrong])]).is_err());
    assert!(project(&[inventory(vec![parent()]), inventory(vec![parent()])]).is_err());
}

#[test]
fn dashboard_repository_duplicate_numeric_or_opaque_identity_is_not_double_counted() {
    let mut second = parent();
    second.number = 8;
    second.node_id = "PR_Second".into();
    assert!(project(&[inventory(vec![parent(), second.clone()])]).is_err());
    second.id = 701;
    second.node_id = "PR_Genuine".into();
    assert!(project(&[inventory(vec![parent(), second.clone()])]).is_err());
    second.node_id = "PR_Second".into();
    assert_eq!(
        project(&[inventory(vec![parent(), second])])
            .unwrap()
            .prs
            .len(),
        2
    );
}

#[test]
fn dashboard_repository_capped_empty_and_unknown_dates_have_honest_partial_coverage() {
    let mut source = inventory(vec![]);
    source.coverage.enumeration_complete = false;
    source.coverage.complete_empty = false;
    source.coverage.status = MetricsReadStatus::Partial;
    source.coverage.reasons = vec!["Requested page budget reached".into()];
    let partial = project(&[source]).unwrap();
    assert!(!partial.pr_coverage.complete_empty);
    assert!(matches!(
        partial.pr_coverage.state,
        CoverageState::Partial(_)
    ));
    let mut pr = parent();
    pr.reasons = vec!["Actual original created_at unavailable".into()];
    let source = inventory(vec![pr]);
    let partial = project(&[source]).unwrap();
    assert!(partial.prs[0].created_at.is_none());
    assert!(matches!(
        partial.pr_coverage.state,
        CoverageState::Partial(_)
    ));
}

#[test]
fn dashboard_repository_limits_retained_metadata_and_summary_before_contributor_cloning() {
    let mut pr = parent();
    pr.title = "x".repeat(256 * 1024 + 1);
    assert!(project(&[inventory(vec![pr])]).is_err());
    let mut pr = parent();
    pr.author.account_id = Some(0);
    assert!(project(&[inventory(vec![pr])]).is_err());
    let mut source = inventory(vec![parent()]);
    source.coverage.reasons = (0..100).map(|n| format!("Source gap {n}")).collect();
    let evidence = project(&[source]).unwrap();
    match evidence.pr_coverage.state {
        CoverageState::Partial(reasons) => {
            assert_eq!(reasons.len(), 65);
            assert!(reasons.last().unwrap().contains("bounded summary"));
        }
        _ => panic!("Known source gaps cannot become complete"),
    }
}
