//! SOURCE ONLY / UNRUN. Genuine provider-produced artifact and live original
//! Core/Session, not a DTO/current-row constructor or replacement budget.
use super::*;
use crate::{
    actions_workspace::ActionsAction,
    scoped_actions_resources_test_support_v2 as r, scoped_actions_test_support_v1 as f,
    scoped_actions_v1::{self as actions, ScopedActionsWorkV1},
    storage::Cache,
    test_support::{Fixture, GhStep},
};
use std::time::{Duration, Instant};
fn tail() -> Vec<GhStep> {
    let mut steps = f::principal(42);
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    steps.extend(f::principal(42));
    steps
}
fn genuine() -> (
    Fixture,
    GithubProvider,
    actions::PreparedScopedActionsV1,
    Arc<ScopedActionsArtifactV2>,
    Vec<GhStep>,
) {
    let mut steps = r::artifacts(vec![r::artifact()]);
    steps.extend(tail());
    let (fixture, provider, mut app, all) = f::opened(steps);
    let prepared = f::read(&mut app, &provider, &fixture, ActionsAction::Artifacts).unwrap();
    let work =
        ScopedActionsWorkV1::capture(prepared.page.clone(), &ActionsAction::OpenArtifact(701), 0)
            .unwrap();
    let target = work
        .resource_v2
        .as_ref()
        .unwrap()
        .archive_target()
        .unwrap()
        .clone();
    (fixture, provider, prepared, target, all)
}
fn limits() -> RepositoryCatalogueAccessLimits {
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = 9 * 1024 * 1024;
    limits.transport.max_document_bytes = 128 * 1024;
    limits.max_repository_reads = 2;
    limits.max_references = 1;
    limits
}
#[test]
fn scoped_actions_resources_v2_shared_core_archive_delegation_keeps_original_clock_cap_and_one_pool(
) {
    let (fixture, p, prepared, target, all) = genuine();
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let token = CancellationToken::default();
    let original_deadline = Instant::now() + Duration::from_secs(30);
    let original = p
        .with_scoped_read_window(original_deadline, 512 * 1024)
        .unwrap();
    let source = prepared.page.source().original();
    let mut session = original
        .begin_repository_catalogue_access(source.source(), &limits(), &token)
        .unwrap();
    session
        .verify(prepared.page.source().reference(), &token)
        .unwrap();
    let started = session.core.deadline();
    assert_eq!(session.core.budget.accounting.requests(), 3);
    let window = session
        .reserve_actions_archive_v2(&target, 1, &token)
        .unwrap();
    assert_eq!(window.max_output_bytes(), 512 * 1024);
    assert_eq!(window.max_requests(), 24);
    assert!(window.capacity_bytes() <= 8 * 1024 * 1024);
    assert!(window.deadline() <= original_deadline);
    assert_eq!(window.deadline(), started.min(original_deadline));
    assert_eq!(
        session.core.deadline(),
        started,
        "Same original Core clock remains, never restarted by private delegation"
    );
    assert_eq!(
        session.core.budget.accounting.requests(),
        3,
        "No actual opening proof or budget reset"
    );
    assert!(session.core.measured_external.is_some());
    assert_eq!(session.core.external_requests_reserved, 24);
    assert!(session
        .reserve_measured_external_reads(1, 1024, 1024, 0, &token)
        .is_err());
    assert!(session
        .reserve_actions_archive_v2(&target, 1, &token)
        .is_err());
    drop(window);
    session
        .verify(prepared.page.source().reference(), &token)
        .unwrap();
    let closed = session
        .close_with_guard(&token, || {
            Cache::open(&f::database(&fixture))
                .map_err(|_| error(GithubErrorKind::IdentityChanged, "Original SQL unavailable"))?
                .validate_repository_catalogue_intent_source(source)
                .map_err(|_| error(GithubErrorKind::IdentityChanged, "Original SQL changed"))
        })
        .unwrap();
    let ledger = closed.external_read_accounting().unwrap();
    assert!(ledger.sealed());
    assert_eq!(ledger.children_started(), 0);
    assert_eq!(ledger.max_requests(), 24);
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_resources_v2_shared_core_refuses_actual_foreign_runner_before_any_archive_pool() {
    let (fixture, _p, prepared, target, all) = genuine();
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let foreign = f::provider(&fixture);
    let token = CancellationToken::default();
    let source = prepared.page.source().original();
    let mut session = foreign
        .begin_repository_catalogue_access(source.source(), &limits(), &token)
        .unwrap();
    session
        .verify(prepared.page.source().reference(), &token)
        .unwrap();
    let deadline = session.core.deadline();
    assert!(session
        .reserve_actions_archive_v2(&target, 1, &token)
        .is_err());
    assert!(session.core.measured_external.is_none());
    assert_eq!(session.core.external_requests_reserved, 0);
    assert_eq!(session.core.deadline(), deadline);
    assert_eq!(session.core.budget.accounting.requests(), 3);
    session
        .verify(prepared.page.source().reference(), &token)
        .unwrap();
    let closed = session
        .close_with_guard(&token, || {
            Cache::open(&f::database(&fixture))
                .map_err(|_| error(GithubErrorKind::IdentityChanged, "Original SQL unavailable"))?
                .validate_repository_catalogue_intent_source(source)
                .map_err(|_| error(GithubErrorKind::IdentityChanged, "Original SQL changed"))
        })
        .unwrap();
    assert!(closed.external_read_accounting().is_none());
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &all);
}
