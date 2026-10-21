//! TASK-105 contracts: a confirmed review write's fence is re-admitted only by
//! an explicit Refresh for the SAME host + numeric account + repository scope,
//! principal first; every changed identity keeps the fence and says why.
use super::*;
use crate::{active_remote_scope_v1::ActiveRemoteScopeV1, github_workspace::GithubWorkspace};

fn identity(login: &str, id: u64) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: login.into(),
        account_id: Some(id),
        gh_version: "2.101.0".into(),
    }
}
fn repositories(repositories: &[&str]) -> PersonalRepositoryScopeV105 {
    PersonalRepositoryScopeV105 {
        host: "github.invalid".into(),
        repositories: repositories.iter().map(|r| r.to_string()).collect(),
    }
}
/// The Personal state just before a confirmed write: Default Personal scope on
/// the workspace's own account, no catalogue source.
fn personal(login: &str, id: u64) -> App {
    let mut app = App::new();
    app.view = View::Inbox;
    app.github = Some(GithubWorkspace::empty(
        identity(login, id),
        vec![],
        Default::default(),
    ));
    app.remote_scope = Some(Arc::new(
        ActiveRemoteScopeV1::default_personal(&identity(login, id)).unwrap(),
    ));
    app.personal_repository_scope_v105 = repositories(&[]);
    app
}
/// What a confirmed write does: capture, fence a shared allocation, retire.
fn confirmed_write(app: &mut App) {
    let capture = capture(app);
    assert!(
        capture.is_some(),
        "Default Personal on its own account qualifies"
    );
    let held_by_worker = app.remote_scope.clone();
    Arc::make_mut(app.remote_scope.as_mut().unwrap()).fence(RemoteScopeFenceV1::SourceUnavailable);
    drop(held_by_worker);
    app.github = None;
    record(app, capture);
    assert!(app.personal_readmission_v105.is_some());
}
fn refresh(app: &mut App, scope: &PersonalRepositoryScopeV105) -> ReadmissionV105 {
    app.refresh_requested = true;
    take_refresh(app, scope)
}

#[test]
fn personal_readmission_v105_same_account_refresh_submits_principal_first_request() {
    let mut app = personal("octocat", 42);
    confirmed_write(&mut app);
    let fenced = app.remote_scope.clone().unwrap();
    let ReadmissionV105::Submit(request) = refresh(&mut app, &repositories(&[])) else {
        panic!("same host/account/repository scope is re-admitted");
    };
    assert!(
        !app.refresh_requested,
        "Refresh consumed by the re-admission"
    );
    assert_eq!(request.host(), "github.invalid");
    assert!(request.repositories.is_empty());
    // A renamed login on the same numeric account is the same principal.
    assert!(request.verify(&identity("octocat-renamed", 42)).is_ok());
    let changed = request.verify(&identity("bob", 43)).unwrap_err();
    assert!(changed.contains("Account changed"), "{changed}");
    assert!(changed.contains("octocat → bob (#43)"), "{changed}");
    assert!(changed.contains("no queue query sent"), "{changed}");
    let mut missing = identity("octocat", 42);
    missing.account_id = None;
    assert_eq!(request.verify(&missing).unwrap_err(), PRINCIPAL_UNAVAILABLE);
    let mut other_host = identity("octocat", 42);
    other_host.host = "ghe.invalid".into();
    assert!(request.verify(&other_host).is_err());

    // In flight: the fence still stands until the verified result installs.
    app.loading[View::Inbox.index()] = true;
    assert!(in_flight(&app));
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &fenced));
    assert!(matches!(
        refresh(&mut app, &repositories(&[])),
        ReadmissionV105::Refused
    ));
    assert!(accept(&mut app, request.admitted()));
    assert!(
        app.remote_scope.is_none(),
        "restart-equivalent pre-scope state"
    );
    assert!(
        app.personal_readmission_v105.is_none(),
        "ticket consumed once"
    );
    // A second write in the same session can fence and re-admit again.
    app.loading[View::Inbox.index()] = false;
    app.github = Some(GithubWorkspace::empty(
        identity("octocat", 42),
        vec![],
        Default::default(),
    ));
    app.remote_scope = Some(Arc::new(
        ActiveRemoteScopeV1::default_personal(&identity("octocat", 42)).unwrap(),
    ));
    confirmed_write(&mut app);
    assert!(matches!(
        refresh(&mut app, &repositories(&[])),
        ReadmissionV105::Submit(_)
    ));
}

#[test]
fn personal_readmission_v105_changed_account_result_keeps_fence_and_reason() {
    let mut app = personal("octocat", 42);
    confirmed_write(&mut app);
    let fenced = app.remote_scope.clone().unwrap();
    let ReadmissionV105::Submit(request) = refresh(&mut app, &repositories(&[])) else {
        panic!("submitted");
    };
    app.loading[View::Inbox.index()] = true;
    let reason = request.verify(&identity("bob", 43)).unwrap_err();
    assert!(!accept(&mut app, request.refused(reason.clone())));
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &fenced));
    assert_eq!(app.status, reason);
    assert_eq!(app.context, reason);
    // Later Refreshes repeat the reason and send nothing.
    app.loading[View::Inbox.index()] = false;
    app.status.clear();
    assert!(matches!(
        refresh(&mut app, &repositories(&[])),
        ReadmissionV105::Refused
    ));
    assert_eq!(app.status, reason);
    assert!(!app.refresh_requested);
}

#[test]
fn personal_readmission_v105_changed_repository_scope_or_host_stays_fenced() {
    let mut app = personal("octocat", 42);
    confirmed_write(&mut app);
    assert!(matches!(
        refresh(&mut app, &repositories(&["example/other"])),
        ReadmissionV105::Refused
    ));
    assert!(app
        .status
        .starts_with("Repository scope changed since the review write"));
    assert!(app.status.contains("no query sent"));
    assert!(!app.refresh_requested);
    // Even reverting the scope does not re-admit a refused ticket.
    assert!(matches!(
        refresh(&mut app, &repositories(&[])),
        ReadmissionV105::Refused
    ));
    assert!(app
        .remote_scope
        .as_ref()
        .unwrap()
        .blocked_intent()
        .is_some());

    let mut app = personal("octocat", 42);
    confirmed_write(&mut app);
    let mut host = repositories(&[]);
    host.host = "ghe.invalid".into();
    assert!(matches!(refresh(&mut app, &host), ReadmissionV105::Refused));
    assert!(app.status.contains("GitHub host changed"), "{}", app.status);
    // Uppercase host spelling is the same host.
    let mut app = personal("octocat", 42);
    confirmed_write(&mut app);
    let mut upper = repositories(&[]);
    upper.host = "GitHub.Invalid".into();
    assert!(matches!(
        refresh(&mut app, &upper),
        ReadmissionV105::Submit(_)
    ));
}

#[test]
fn personal_readmission_v105_only_the_confirmed_writes_own_fence_qualifies() {
    // Not Default Personal: an unavailable/fenced scope never gets a ticket.
    let mut app = personal("octocat", 42);
    app.remote_scope = Some(Arc::new(ActiveRemoteScopeV1::unavailable(
        RemoteScopeFenceV1::PrincipalUnavailable,
    )));
    assert!(capture(&app).is_none());
    // The workspace is on another account than the scope's original.
    let mut app = personal("octocat", 42);
    app.github = Some(GithubWorkspace::empty(
        identity("bob", 43),
        vec![],
        Default::default(),
    ));
    assert!(capture(&app).is_none());
    // No scope at all (legacy startup) has nothing to re-admit.
    let mut app = personal("octocat", 42);
    app.remote_scope = None;
    assert!(capture(&app).is_none());
    // A capture without the SourceUnavailable fence records nothing.
    let mut app = personal("octocat", 42);
    let early = capture(&app);
    record(&mut app, early);
    assert!(app.personal_readmission_v105.is_none());

    // A later, different fence supersedes the ticket: plain stop applies.
    let mut app = personal("octocat", 42);
    confirmed_write(&mut app);
    let stale = app.remote_scope.clone().unwrap();
    let ReadmissionV105::Submit(request) = refresh(&mut app, &repositories(&[])) else {
        panic!("submitted");
    };
    Arc::make_mut(app.remote_scope.as_mut().unwrap()).fence(RemoteScopeFenceV1::PrincipalChanged);
    assert!(!Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &stale));
    app.loading[View::Inbox.index()] = true;
    assert!(!in_flight(&app));
    assert!(
        !accept(&mut app, request.admitted()),
        "stale result installs nothing"
    );
    assert!(app.remote_scope.is_some());
    assert!(matches!(
        refresh(&mut app, &repositories(&[])),
        ReadmissionV105::NotApplicable
    ));
    assert!(app.refresh_requested, "left for the ordinary fenced stop");
    assert!(app.personal_readmission_v105.is_none());
    // Refresh on another view is never intercepted.
    let mut app = personal("octocat", 42);
    confirmed_write(&mut app);
    app.view = View::Actions;
    assert!(matches!(
        refresh(&mut app, &repositories(&[])),
        ReadmissionV105::NotApplicable
    ));
}
