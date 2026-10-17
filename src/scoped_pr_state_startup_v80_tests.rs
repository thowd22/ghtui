//! Genuine real Background confirmation and authenticated startup issuers.
//! Neither a journal ID nor missing scope options construct Source authority.
use super::*;
#[path = "scoped_pr_state_complete_source_v80_tests.rs"]
mod complete_source_v80;
use crate::{
    active_remote_scope_v1::{ActiveRemoteScopeV1, RemoteScopeFenceV1},
    github_workspace::{GithubAction, GithubWorkspace},
    state_inbox_sync_v76::{StateSourceOriginV80, StateSourceSyncV76},
};

fn confirmed() -> (Setup, Background, DraftWriter) {
    let (mut s, mut bg, writer, _) = background_ready(false);
    submit(&mut s, &mut bg, &writer, FormAction::Submit);
    finish(&mut s, &mut bg);
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .state,
        "closed"
    );
    assert!(s
        .app
        .github
        .as_ref()
        .unwrap()
        .state_membership_unavailable_v68());
    assert_calls(&s, s.steps.len());
    (s, bg, writer)
}
fn back(s: &mut Setup) {
    let sql = all_sql(s);
    let calls = strict_calls(s).len();
    s.app.dispatch(Action::Github(GithubAction::Back));
    crate::personal_pr_entry_v13::retire_invalid(&mut s.app);
    crate::notification_pr_v63::retire_invalid(&mut s.app);
    let g = s.app.github.as_ref().unwrap();
    assert!(g.pr.is_none() && g.scoped_pr_navigation.is_none() && g.scoped_pr_resource.is_none());
    assert!(g.personal_pr_page_ticket_v13.is_none() && g.notification_pr_page_ticket_v63.is_none());
    assert!(s.app.personal_pr_source_v13.is_none() && s.app.notification_pr_source_v63.is_none());
    assert!(g.state_membership_unavailable_v68() && g.inbox.pull_requests.is_empty());
    assert!(g.coverage_label().contains("counts unavailable"));
    assert_eq!(all_sql(s), sql);
    assert_calls(s, calls);
}
fn unchanged_local(s: &Setup, sql: &[String], calls: usize) {
    assert_eq!(all_sql(s).as_slice(), sql);
    assert_calls(s, calls);
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (name, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(name)).unwrap(), expected);
    }
}

#[test]
fn pr_lifecycle_v68_actual_state_confirm_back_retains_only_positive_legacy_queue_read_intent() {
    let (mut s, _bg, _writer) = confirmed();
    assert!(s.app.remote_scope.is_none() && s.app.remote_scope_source.is_none());
    back(&mut s);
    let stable = all_sql(&s);
    let calls = s.steps.len();
    let sync = StateSourceSyncV76::capture(&s.app, &database(&s.fixture)).unwrap();
    assert!(matches!(
        sync.original().origin(),
        StateSourceOriginV80::LegacyStartup
    ));
    assert!(sync.owns(&s.app));
    sync.validate_original().unwrap();
    unchanged_local(&s, &stable, calls);
    // Ordinary non-State Back retains its existing page-ticket semantics.
    let mut ordinary = setup(vec![]);
    let ticket = ordinary
        .app
        .github
        .as_ref()
        .unwrap()
        .personal_pr_page_ticket_v13
        .clone()
        .unwrap();
    let original = sql(&ordinary.fixture);
    ordinary.app.dispatch(Action::Github(GithubAction::Back));
    assert!(Arc::ptr_eq(
        &ticket,
        ordinary
            .app
            .github
            .as_ref()
            .unwrap()
            .personal_pr_page_ticket_v13
            .as_ref()
            .unwrap()
    ));
    assert!(!ordinary
        .app
        .github
        .as_ref()
        .unwrap()
        .state_membership_unavailable_v68());
    assert_eq!(sql(&ordinary.fixture), original);
    assert_eq!(strict_calls(&ordinary).len(), ordinary.steps.len());
}

#[test]
fn pr_lifecycle_v68_authenticated_state_startup_after_clear_captures_real_absence_without_private_rows(
) {
    let (mut s, _bg, _writer) = confirmed();
    back(&mut s);
    let scope = s.app.github.as_ref().unwrap().scope().unwrap();
    let source = s.app.github.as_ref().unwrap().source();
    Cache::open(&database(&s.fixture))
        .unwrap()
        .clear_records(&scope)
        .unwrap();
    let cache = Cache::open(&database(&s.fixture)).unwrap();
    assert!(cache.sync_checkpoint(&scope, &source).unwrap().is_none());
    assert!(
        cache.sync_frame(&scope, &source).is_err(),
        "Ordinary payload guard remains exact"
    );
    let stable = all_sql(&s);
    let before = s.steps.len();
    s.steps.extend(principal());
    s.fixture.scripted_gh(&s.steps);
    let startup = GithubWorkspace::legacy_startup_v80(
        &s.api,
        vec![],
        Default::default(),
        &database(&s.fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(
        startup.generation, 0,
        "Display absence, NEVER synthesized SQL reservation"
    );
    assert!(startup.pr.is_none() && startup.inbox.pull_requests.is_empty());
    assert!(startup.state_membership_unavailable_v68());
    assert!(startup.legacy_queue_v80().is_some());
    let app = App {
        view: View::Inbox,
        github: Some(startup),
        ..Default::default()
    };
    let sync = StateSourceSyncV76::capture(&app, &database(&s.fixture)).unwrap();
    assert!(sync.owns(&app));
    sync.validate_original().unwrap();
    assert!(app.remote_scope.is_none() && app.remote_scope_source.is_none());
    assert_eq!(
        s.steps.len(),
        before + 2,
        "SAME actual startup principal; no hidden search/read"
    );
    unchanged_local(&s, &stable, s.steps.len());
    Cache::open(&database(&s.fixture))
        .unwrap()
        .clear_records(&scope)
        .unwrap();
    let cleared = all_sql(&s);
    assert!(
        sync.validate_original().is_err(),
        "Never recapture later Clear as current permission"
    );
    unchanged_local(&s, &cleared, s.steps.len());
}

#[test]
fn pr_lifecycle_v68_none_without_positive_provenance_and_unavailable_fenced_or_newer_scopes_refuse()
{
    let (mut s, _bg, _writer) = confirmed();
    back(&mut s);
    let stable = all_sql(&s);
    let calls = s.steps.len();
    let actual = s.app.github.take().unwrap();
    let mut impostor = GithubWorkspace::empty(
        actual.identity.clone(),
        actual.repositories.clone(),
        actual.exclusions.clone(),
    );
    impostor.generation = actual.generation;
    assert_eq!(impostor.source(), actual.source());
    s.app.github = Some(impostor);
    assert!(s.app.remote_scope.is_none() && s.app.remote_scope_source.is_none());
    assert!(StateSourceSyncV76::capture(&s.app, &database(&s.fixture)).is_err());
    s.app.github = Some(actual);
    for reason in [
        RemoteScopeFenceV1::PrincipalUnavailable,
        RemoteScopeFenceV1::AccessUnavailable,
        RemoteScopeFenceV1::PrivacyFence,
    ] {
        s.app.remote_scope = Some(Arc::new(ActiveRemoteScopeV1::unavailable(reason)));
        assert!(StateSourceSyncV76::capture(&s.app, &database(&s.fixture)).is_err());
    }
    let mut fenced =
        ActiveRemoteScopeV1::default_personal(&s.app.github.as_ref().unwrap().identity).unwrap();
    fenced.fence(RemoteScopeFenceV1::SourceUnavailable);
    s.app.remote_scope = Some(Arc::new(fenced));
    assert!(StateSourceSyncV76::capture(&s.app, &database(&s.fixture)).is_err());
    // Genuine current Catalogue factory supplies both explicit-empty and
    // nonempty opposite origins; missing Source is never a Legacy fallback.
    let owner = json!({"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"});
    s.steps.extend(principal());
    s.steps.push(get("orgs/example", owner.clone()));
    s.steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([repository()]),
    ));
    s.steps.push(get("orgs/example", owner));
    s.steps.extend(principal());
    s.fixture.scripted_gh(&s.steps);
    let read = s
        .api
        .repository_catalogue_read(
            None,
            &crate::providers::gh::RepositoryCatalogueSource::Organization("example".into()),
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    for ids in [&[][..], &[10000][..]] {
        let active = ActiveRemoteScopeV1::explicit_selection(&read, ids).unwrap();
        s.app.remote_scope = Some(Arc::new(active));
        assert!(s.app.remote_scope_source.is_none());
        assert!(StateSourceSyncV76::capture(&s.app, &database(&s.fixture)).is_err());
    }
    s.app.remote_scope = None;
    let current = StateSourceSyncV76::capture(&s.app, &database(&s.fixture)).unwrap();
    s.app.github.as_mut().unwrap().generation += 1;
    assert!(!current.owns(&s.app));
    assert!(StateSourceSyncV76::capture(&s.app, &database(&s.fixture)).is_err());
    assert_eq!(s.steps.len(), calls + 7);
    unchanged_local(&s, &stable, s.steps.len());
}
