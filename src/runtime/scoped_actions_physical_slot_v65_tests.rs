//! Actual shared Background channel. No fabricated Completion/source authority.
use super::*;
use crate::{
    search::{
        corpus::{LocalSource, RepositorySelection},
        workspace::SearchWorkspace,
    },
    storage::Scope,
    workspace::{RepositoryMode, Workspace},
};

fn pending() -> (
    crate::test_support::Fixture,
    App,
    Vec<crate::test_support::GhStep>,
    Arc<crate::scoped_actions_v1::ScopedActionsWorkV1>,
) {
    let (fixture, _, mut app, steps) = f::setup(vec![]);
    assert!(native::intercept(
        &mut app,
        &A::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2
        }
    ));
    let work = app.scoped_actions_work_v1.as_ref().unwrap().clone();
    assert!(native::current_input(&app, &work));
    (fixture, app, steps, work)
}
fn observe(background: &mut Background) -> Completion {
    background
        .completions
        .recv_timeout(Duration::from_secs(8))
        .expect("Actual submitted worker must complete")
}

#[test]
fn scoped_actions_physical_slot_v65_pending_current_actions_only_vetoes_same_slot_not_actual_repository_or_search(
) {
    for view in [View::Actions, View::Repository, View::Search] {
        let (fixture, mut app, steps, work) = pending();
        let source = app.remote_scope_source.as_ref().unwrap().clone();
        let active = app.remote_scope.as_ref().unwrap().clone();
        let actions = app.actions.as_ref().unwrap().clone();
        let original_rows = app.actions.as_ref().unwrap().rows.as_ptr();
        let sql = f::sql_state(&fixture);
        let git = f::git_state(&fixture);
        let mut background = Background::new();
        let directory = fixture.repo();
        assert!(background.submit(&mut app, view, move |cancel| {
            Ok(match view {
                // This is the actual old generic prepared Actions data, NOT a
                // new original/owner. Queued genuine native input must veto it.
                View::Actions => PreparedView {
                    actions: Some(actions),
                    ..Default::default()
                },
                View::Repository => PreparedView {
                    workspace: Workspace::load_with_cancellation(
                        &directory,
                        RepositoryMode::Working,
                        "HEAD",
                        None,
                        cancel,
                    )?,
                    ..Default::default()
                },
                View::Search => {
                    let mut search = SearchWorkspace::new(
                        Scope::new("local", "local").map_err(|e| e.to_string())?,
                        vec![RepositorySelection {
                            repository: "fixture/actual-local".into(),
                            directory,
                            source: LocalSource::Worktree,
                            sets: vec![],
                        }],
                    );
                    search
                        .search("LOCAL_DIRTY_ACTIONS_CANARY", &cancel)
                        .map_err(|e| e.0)?;
                    PreparedView {
                        search: Some(search),
                        ..Default::default()
                    }
                }
                _ => unreachable!(),
            })
        }));
        let slot = background.current[&view.index()].clone();
        let completion = observe(&mut background);
        assert_eq!(completion.view, view);
        assert!(completion.result.is_ok());
        assert!(completion.remote_scope_owner.is_none());
        let status = app.status.clone();
        let context = app.context.clone();
        put(&background, completion);
        assert_eq!(background.poll(&mut app), view != View::Actions);
        assert_eq!(
            background.current.contains_key(&view.index()),
            view == View::Actions
        );
        assert_eq!(app.loading[view.index()], view == View::Actions);
        assert!(Arc::ptr_eq(
            app.scoped_actions_work_v1.as_ref().unwrap(),
            &work
        ));
        if view != View::Actions {
            assert!(native::current_input(&app, &work));
        }
        assert!(!work.cancellation().is_cancelled());
        assert!(!slot.1.is_cancelled());
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &source
        ));
        assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
        assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), original_rows);
        assert_eq!(app.status, status);
        assert_eq!(app.context, context);
        if view == View::Repository {
            assert!(app
                .workspace
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .any(|row| row.label.contains("tracked")));
        }
        if view == View::Search {
            assert!(app
                .search
                .as_ref()
                .unwrap()
                .page
                .as_ref()
                .unwrap()
                .hits
                .iter()
                .any(|hit| hit.document.key.repository == "fixture/actual-local"));
        }
        assert_eq!(f::sql_state(&fixture), sql);
        assert_eq!(f::git_state(&fixture), git);
        f::exact(&fixture, &steps);
        background.cancel(view);
        work.cancellation().cancel();
    }
}

#[test]
fn scoped_actions_physical_slot_v65_pending_actions_allows_actual_catalogue_hidden_slot_five_local_result(
) {
    use crate::repository_catalogue_native::{
        self as catalogue, CataloguePrepared, CataloguePreparedKind, CatalogueUiAction,
    };
    let (fixture, mut app, steps, work) = pending();
    catalogue::dispatch(&mut app, CatalogueUiAction::Open);
    let owner = app.catalogue.as_ref().unwrap().owner.clone();
    assert!(catalogue::owns(&app, &owner));
    let worker_owner = owner.clone();
    let mut background = Background::new();
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    assert!(
        background.submit_catalogue(&mut app, owner.clone(), move |_| Ok(PreparedView {
            catalogue: Some(CataloguePrepared {
                owner: worker_owner,
                kind: CataloguePreparedKind::Message("ACTUAL OWNED LOCAL CATALOGUE RESULT".into())
            }),
            ..Default::default()
        }))
    );
    let token = background.current[&5].1.clone();
    let completion = observe(&mut background);
    assert_eq!(completion.view, View::Actions);
    assert!(completion
        .catalogue_owner
        .as_ref()
        .is_some_and(|actual| Arc::ptr_eq(actual, &owner)));
    assert!(completion.remote_repository_owner.is_none());
    put(&background, completion);
    assert!(background.poll(&mut app));
    assert!(!background.current.contains_key(&5));
    assert!(!token.is_cancelled());
    assert!(catalogue::owns(&app, &owner));
    assert!(
        matches!(&app.catalogue.as_ref().unwrap().dialog,Some(crate::repository_catalogue_native::CatalogueDialog::Message{reason,..})if reason=="ACTUAL OWNED LOCAL CATALOGUE RESULT")
    );
    assert!(Arc::ptr_eq(
        app.scoped_actions_work_v1.as_ref().unwrap(),
        &work
    ));
    assert!(!work.cancellation().is_cancelled());
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &steps);
    work.cancellation().cancel();
}

#[test]
fn scoped_actions_physical_slot_v65_pending_actions_allows_actual_remote_hidden_slot_six_fresh_provider_result(
) {
    use crate::{
        remote_repository_native_v1 as remote, remote_repository_v1::RemoteBrowseActionV1,
        remote_repository_v1_test_support as fx,
    };
    let (fixture, mut app, steps, work) = pending();
    let remote_fixture = crate::test_support::Fixture::new();
    let objects = fx::objects(&remote_fixture);
    let mut expected = fx::source_steps();
    expected.extend(fx::prefix());
    expected.extend(fx::default_payload(&objects));
    expected.extend(fx::suffix(42));
    expected.extend(fx::prefix());
    expected.push(fx::get(
        "repos/example/remote/branches?per_page=100&page=1",
        serde_json::json!([{"name":"main","commit":{"sha":objects.commit}}]),
    ));
    expected.extend(fx::suffix(42));
    // This slot-separation contract does not introduce a login-rename case.
    // All actual raw/closing users retain the same positive UID and login.
    for step in &mut expected {
        if step.args.get(1).is_some_and(|s| s == "user") {
            let mut raw: serde_json::Value = serde_json::from_slice(&step.stdout).unwrap();
            raw["login"] = serde_json::json!("octocat");
            step.stdout = serde_json::to_vec(&raw).unwrap();
        }
    }
    remote_fixture.scripted_gh(&expected);
    let source = fx::source(&remote_fixture);
    let provider = fx::provider(&remote_fixture);
    let database = remote_fixture.root.join("cache.sqlite3");
    let state = remote::initial(source, 0, &provider, &database, &Default::default()).unwrap();
    remote::install_initial(&mut app, state);
    remote::dispatch(&mut app, RemoteBrowseActionV1::Branches);
    let request = remote::take_request(&mut app).unwrap();
    let owner = request.owner.clone();
    assert!(owner.owns(&app));
    let mut background = Background::new();
    let token_sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    assert!(
        background.submit_remote_repository_v1(&mut app, owner.clone(), move |cancel| {
            remote::execute(request, &provider, &database, &cancel)
        })
    );
    let token = background.current[&6].1.clone();
    let completion = observe(&mut background);
    assert_eq!(completion.view, View::Repository);
    assert!(completion.result.is_ok());
    assert!(completion
        .remote_repository_owner
        .as_ref()
        .is_some_and(|actual| Arc::ptr_eq(actual, &owner)));
    put(&background, completion);
    assert!(background.poll(&mut app));
    assert!(!background.current.contains_key(&6));
    assert!(!token.is_cancelled());
    assert!(app.remote_repository.is_some());
    assert!(
        !owner.owns(&app),
        "Actual completed request no longer owns pending permission"
    );
    assert!(Arc::ptr_eq(
        app.scoped_actions_work_v1.as_ref().unwrap(),
        &work
    ));
    assert!(!work.cancellation().is_cancelled());
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| remote::draw(frame, &mut app))
        .unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    assert!(rendered.contains("main"));
    assert!(rendered.contains("Branches"));
    assert_eq!(f::sql_state(&fixture), token_sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &steps);
    fx::exact(&remote_fixture, &expected);
    fx::preserved(&remote_fixture, &objects);
    work.cancellation().cancel();
}
