//! Real original Native Case and actual Background channels, not fabricated
//! completions or transport/source permission. The pending native capture has
//! not been consumed by main, so it does not yet have a native worker token.
use super::*;
use crate::{
    runtime::PreparedView,
    search::{
        corpus::{LocalSource, RepositorySelection},
        workspace::SearchWorkspace,
    },
    storage::Scope,
    workspace::{RepositoryMode, Workspace},
};

fn pending() -> Case {
    let mut case = Case::new(true);
    native::open_images(&mut case.app);
    assert!(case.app.pr_native_diff_work_v1.is_some());
    assert!(case
        .app
        .pr_native_diff_work_v1
        .as_ref()
        .unwrap()
        .matches(&case.app));
    case
}

#[test]
fn scoped_pr_native_diff_physical_slot_v65_actual_inbox_success_error_panic_stay_vetoed_while_repository_and_search_progress(
) {
    for (view, outcome) in [
        (View::Inbox, 0),
        (View::Inbox, 1),
        (View::Inbox, 2),
        (View::Repository, 0),
        (View::Search, 0),
    ] {
        let mut case = pending();
        let source = case.app.remote_scope_source.as_ref().unwrap().clone();
        let active = case.app.remote_scope.as_ref().unwrap().clone();
        let proof = case
            .app
            .pr_file_images
            .as_ref()
            .unwrap()
            .original_proof_v1()
            .clone();
        let work = case.app.pr_native_diff_work_v1.as_ref().unwrap().clone();
        let directory = case.fixture.repo();
        let expected_directory = directory.clone();
        let mut background = Background::new();
        assert!(background.submit(&mut case.app, view, move |cancel| {
            if outcome == 1 {
                return Err("ACTUAL_OLD_INBOX_ERROR_MUST_NOT_PUBLISH".into());
            }
            assert_ne!(outcome, 2, "Actual old generic worker caught panic");
            Ok(match view {
                View::Inbox => PreparedView {
                    lines: vec!["ACTUAL_OLD_INBOX_SUCCESS_MUST_NOT_PUBLISH".into()],
                    clear_private: true,
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
                            repository: "fixture/actual-native-local".into(),
                            directory,
                            source: LocalSource::Worktree,
                            sets: vec![],
                        }],
                    );
                    search.search("context", &cancel).map_err(|e| e.0)?;
                    PreparedView {
                        search: Some(search),
                        ..Default::default()
                    }
                }
                _ => unreachable!(),
            })
        }));
        let status = case.app.status.clone();
        let context = case.app.context.clone();
        let inbox = case.app.content[View::Inbox.index()].clone();
        channel::assert_actual_physical_slot_v65(
            &mut background,
            &mut case.app,
            view.index(),
            view != View::Inbox,
            outcome == 0,
        );
        assert_eq!(case.app.loading[view.index()], view == View::Inbox);
        assert!(Arc::ptr_eq(
            case.app.remote_scope_source.as_ref().unwrap(),
            &source
        ));
        assert!(Arc::ptr_eq(
            case.app.remote_scope.as_ref().unwrap(),
            &active
        ));
        assert!(Arc::ptr_eq(
            case.app
                .pr_file_images
                .as_ref()
                .unwrap()
                .original_proof_v1(),
            &proof
        ));
        assert!(Arc::ptr_eq(
            case.app.pr_native_diff_work_v1.as_ref().unwrap(),
            &work
        ));
        assert!(work.matches(&case.app));
        assert_eq!(case.app.status, status);
        assert_eq!(case.app.context, context);
        assert_eq!(case.app.content[View::Inbox.index()], inbox);
        if view == View::Repository {
            assert_eq!(
                case.app.workspace.as_ref().unwrap().directory,
                expected_directory
            );
            assert_eq!(
                case.app.workspace.as_ref().unwrap().mode,
                RepositoryMode::Working
            );
        }
        if view == View::Search {
            assert!(case
                .app
                .search
                .as_ref()
                .unwrap()
                .page
                .as_ref()
                .unwrap()
                .hits
                .iter()
                .any(|hit| hit.document.key.repository == "fixture/actual-native-local"));
        }
        case.intact();
        background.cancel(view);
    }
}

#[test]
fn scoped_pr_native_diff_physical_slot_v65_actual_hidden_catalogue_five_publishes_without_consuming_native_input(
) {
    use crate::repository_catalogue_native::{
        self as catalogue, CataloguePrepared, CataloguePreparedKind, CatalogueUiAction,
    };
    let mut case = pending();
    let work = case.app.pr_native_diff_work_v1.as_ref().unwrap().clone();
    // Select the host actually observed by the genuine Case. The App default
    // is github.com, which must not impersonate this github.invalid identity.
    case.app.settings_host = case.app.github.as_ref().unwrap().identity.host.clone();
    catalogue::dispatch(&mut case.app, CatalogueUiAction::Open);
    let owner = case.app.catalogue.as_ref().unwrap().owner.clone();
    assert!(catalogue::owns(&case.app, &owner));
    let worker_owner = owner.clone();
    let mut background = Background::new();
    assert!(
        background.submit_catalogue(&mut case.app, owner.clone(), move |_| Ok(PreparedView {
            catalogue: Some(CataloguePrepared {
                owner: worker_owner,
                kind: CataloguePreparedKind::Message("ACTUAL_NATIVE_INDEPENDENT_CATALOGUE".into()),
            }),
            ..Default::default()
        }))
    );
    channel::assert_actual_physical_slot_v65(&mut background, &mut case.app, 5, true, true);
    assert!(catalogue::owns(&case.app, &owner));
    assert!(
        matches!(&case.app.catalogue.as_ref().unwrap().dialog,Some(crate::repository_catalogue_native::CatalogueDialog::Message{reason,..})if reason=="ACTUAL_NATIVE_INDEPENDENT_CATALOGUE")
    );
    assert!(Arc::ptr_eq(
        case.app.pr_native_diff_work_v1.as_ref().unwrap(),
        &work
    ));
    case.intact();
}

#[test]
fn scoped_pr_native_diff_physical_slot_v65_actual_remote_six_full_provider_read_publishes_without_consuming_native_input(
) {
    use crate::{
        remote_repository_native_v1 as remote, remote_repository_v1::RemoteBrowseActionV1,
        remote_repository_v1_test_support as fx,
    };
    let mut case = pending();
    let work = case.app.pr_native_diff_work_v1.as_ref().unwrap().clone();
    let other = crate::test_support::Fixture::new();
    let objects = fx::objects(&other);
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
    for step in &mut expected {
        if step.args.get(1).is_some_and(|s| s == "user") {
            let mut raw: serde_json::Value = serde_json::from_slice(&step.stdout).unwrap();
            raw["login"] = serde_json::json!("octocat");
            step.stdout = serde_json::to_vec(&raw).unwrap();
        }
    }
    other.scripted_gh(&expected);
    let source = fx::source(&other);
    let provider = fx::provider(&other);
    let database = other.root.join("cache.sqlite3");
    let state = remote::initial(source, 0, &provider, &database, &Default::default()).unwrap();
    remote::install_initial(&mut case.app, state);
    remote::dispatch(&mut case.app, RemoteBrowseActionV1::Branches);
    let request = remote::take_request(&mut case.app).unwrap();
    let owner = request.owner.clone();
    assert!(owner.owns(&case.app));
    let mut background = Background::new();
    assert!(
        background.submit_remote_repository_v1(&mut case.app, owner.clone(), move |cancel| {
            remote::execute(request, &provider, &database, &cancel)
        })
    );
    channel::assert_actual_physical_slot_v65(&mut background, &mut case.app, 6, true, true);
    assert!(!owner.owns(&case.app));
    assert!(Arc::ptr_eq(
        case.app.pr_native_diff_work_v1.as_ref().unwrap(),
        &work
    ));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| remote::draw(frame, &mut case.app))
        .unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(rendered.contains("Branches"));
    assert!(rendered.contains("main"));
    case.intact();
    fx::exact(&other, &expected);
    fx::preserved(&other, &objects);
}
