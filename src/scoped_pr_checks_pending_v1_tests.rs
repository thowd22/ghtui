//! Genuine original Checks sources, actual provider closure and live queue
//! barriers supplement the real-binary pending Back/cancel journeys.
use super::*;
use crate::github_workspace::GithubAction;

fn rows(app: &App) -> *const crate::github_workspace::PrRow {
    app.github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .rows
        .as_ptr()
}

#[test]
fn scoped_check_pending_capture_and_closed_install_keep_actual_page_and_inbox_rows() {
    let steps = whole(&bridge_steps(REPO));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let page_rows = rows(&app);
    let inbox = app.github.as_ref().unwrap().inbox.pull_requests.as_ptr();
    let captured = input(&mut app);
    assert_eq!(
        rows(&app),
        page_rows,
        "The worker never takes or clones the actual Checks rows"
    );
    assert!(pr_checks_native::owns_origin(&app, captured.origin()));
    let provider = provider(&fixture);
    let result = load_v1(
        captured,
        &provider,
        &database(&fixture),
        &Default::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(rows(&app), page_rows);
    install(&mut app, result);
    assert_eq!(rows(&app), page_rows);
    assert_eq!(
        app.github.as_ref().unwrap().inbox.pull_requests.as_ptr(),
        inbox
    );
    assert_eq!(app.view, View::Actions);
    app.dispatch(Action::PrChecks(pr_checks_native::PrChecksUiAction::Back));
    assert_eq!(app.view, View::Inbox);
    assert_eq!(rows(&app), page_rows);
    read_only(&fixture, &before, &git, &steps);
}

#[test]
fn scoped_check_pending_genuine_closed_result_cannot_reopen_after_original_back() {
    let steps = whole(&bridge_steps(REPO));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let inbox = app.github.as_ref().unwrap().inbox.pull_requests.as_ptr();
    let prepared = load_bridge(&fixture, &mut app, &Default::default());
    assert!(app.github.as_ref().unwrap().pr.is_some());
    app.dispatch(Action::Github(GithubAction::Back));
    assert!(app.pr_checks_origin.is_none());
    assert!(app.github.as_ref().unwrap().pr.is_none());
    let status = app.status.clone();
    let context = app.context.clone();
    assert!(install_v1(&mut app, *prepared.scoped_pr_checks.unwrap()).is_err());
    assert_eq!(app.status, status);
    assert_eq!(app.context, context);
    assert_eq!(
        app.github.as_ref().unwrap().inbox.pull_requests.as_ptr(),
        inbox
    );
    assert!(app.actions.is_none());
    assert!(app.pr_checks_route.is_none());
    assert_eq!(app.view, View::Inbox);
    read_only(&fixture, &before, &git, &steps);
}

#[test]
fn scoped_check_pending_missing_actual_page_is_not_an_owned_request() {
    let steps = whole(&[]);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let captured = input(&mut app);
    let origin = captured.origin().clone();
    app.github.as_mut().unwrap().pr = None;
    assert!(!pr_checks_native::owns_origin(&app, &origin));
    let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let work_ran = ran.clone();
    let mut background = Background::new();
    assert!(
        !background.submit_scoped_pr_check_v1(&mut app, View::Inbox, origin, move |_| {
            work_ran.store(true, std::sync::atomic::Ordering::SeqCst);
            Err("MISSING_PAGE_WORK_MUST_NOT_RUN".into())
        })
    );
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
    read_only(&fixture, &before, &git, &steps);
}

#[test]
fn scoped_check_pending_saturated_actual_queue_keeps_page_and_never_starts_private_read() {
    let steps = whole(&[]);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let page_rows = rows(&app);
    let captured = input(&mut app);
    assert_eq!(rows(&app), page_rows);
    let origin = captured.origin().clone();
    let mut background = Background::new();
    let mut releases = Vec::new();
    for _ in 0..2 {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        assert!(background.submit(&mut app, View::Search, move |_| {
            ready_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(8)).unwrap();
            Err("Unrelated queue barrier released".into())
        }));
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        releases.push(release_tx);
    }
    for _ in 0..4 {
        assert!(background.submit(&mut app, View::Dashboard, |_| Err(
            "Unrelated queued work".into()
        )));
    }
    let provider = provider(&fixture);
    let database = database(&fixture);
    let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let work_ran = ran.clone();
    let accepted =
        background.submit_scoped_pr_check_v1(&mut app, View::Inbox, origin, move |cancel| {
            work_ran.store(true, std::sync::atomic::Ordering::SeqCst);
            load_v1(captured, &provider, &database, &Default::default(), &cancel)
        });
    for release in releases {
        release.send(()).unwrap();
    }
    assert!(!accepted);
    assert!(app.status.contains("queue is full"));
    assert!(!app.loading[View::Inbox.index()]);
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(rows(&app), page_rows);
    app.dispatch(Action::Github(GithubAction::Back));
    assert!(app.github.as_ref().unwrap().pr.is_none());
    read_only(&fixture, &before, &git, &steps);
}
