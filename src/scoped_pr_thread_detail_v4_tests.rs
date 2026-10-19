//! UNRUN: actual Source/PR admission, genuine Summary/Timeline projection,
//! same original measured provider and Background; no dummy Files resource.
use super::*;
use crate::scoped_pr_observed_tabs_v1::{install_if_current, prepare, take_input, ObservedPrTabV1};

fn detail_real(timeline: bool, extra: Vec<GhStep>) -> Real {
    let mut steps = script(&[]);
    steps.extend(extra);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, active, mut github) = opened(&fixture);
    assert!(github.scoped_pr_resource.is_none());
    if timeline {
        let input = take_input(&mut github, ObservedPrTabV1::Timeline).unwrap();
        let prepared = prepare(source.clone(), active.clone(), input, &Default::default()).unwrap();
        install_if_current(&mut github, prepared).unwrap();
    }
    let app = App {
        view: View::Inbox,
        remote_scope_source: Some(source),
        remote_scope: Some(active),
        github: Some(github),
        inbox_list: crate::ui_table::ListState {
            selected: 0,
            top: 0,
        },
        ..Default::default()
    };
    let sql = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    Real {
        fixture,
        app,
        steps,
        sql,
        git,
    }
}
fn thread_finish(real: &mut Real, bg: &mut Background) {
    let owner = real.app.scoped_threads_work_v1.take().unwrap();
    assert!(owner.files_intent().is_none());
    assert!(owner.browse_intent().unwrap().selected_path_v4().is_none());
    assert!(bg.submit_scoped_threads_v1(
        &mut real.app,
        owner,
        provider(&real.fixture),
        database(&real.fixture)
    ));
    let deadline = Instant::now() + Duration::from_secs(8);
    while real.app.loading[View::Inbox.index()] {
        bg.poll(&mut real.app);
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
fn paint(app: &mut App) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(120)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn review_threads_v4_real_summary_timeline_key_click_all_threads_and_pure_back_have_no_files_or_event_claim(
) {
    for timeline in [false, true] {
        for mouse in [false, true] {
            let mut real = detail_real(timeline, thread_steps(Some("LINE")));
            let source = real.app.remote_scope_source.as_ref().unwrap().clone();
            let nav = real
                .app
                .github
                .as_ref()
                .unwrap()
                .scoped_pr_navigation
                .as_ref()
                .unwrap()
                .clone();
            let before = paint(&mut real.app);
            assert!(before.contains("T All threads") && !before.contains("[T All threads]"));
            if mouse {
                let point = real
                    .app
                    .hits
                    .iter()
                    .find(|hit| {
                        hit.action
                            == Action::Github(
                                crate::github_workspace::GithubAction::ReviewThreadsV4,
                            )
                    })
                    .unwrap()
                    .rect;
                real.app.handle_event(crossterm::event::Event::Mouse(
                    crossterm::event::MouseEvent {
                        kind: crossterm::event::MouseEventKind::Down(
                            crossterm::event::MouseButton::Left,
                        ),
                        column: point.x,
                        row: point.y,
                        modifiers: crossterm::event::KeyModifiers::NONE,
                    },
                ));
            } else {
                real.app.handle_event(crossterm::event::Event::Key(
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Char('T'),
                        crossterm::event::KeyModifiers::NONE,
                    ),
                ));
            }
            let mut bg = Background::new();
            thread_finish(&mut real, &mut bg);
            let view = real.app.scoped_threads_v1.as_ref().unwrap();
            assert!(!view.file_only);
            let prepared = view.prepared.as_ref().unwrap();
            assert!(prepared.selected_file.is_empty());
            assert_eq!(prepared.all, [0]);
            assert_eq!(prepared.parent_v4.node_id(), "GENUINE_PR_NODE_1007");
            assert!(prepared
                .header
                .iter()
                .any(|line| line.contains("ALL PR") && line.contains("no file/event association")));
            ui_v1::dispatch(&mut real.app, ui_v1::ThreadUiActionV1::Filter);
            assert!(!real.app.scoped_threads_v1.as_ref().unwrap().file_only);
            native_v1::close(&mut real.app);
            let after = paint(&mut real.app);
            assert_eq!(after.lines().take(38).collect::<Vec<_>>(),before.lines().take(38).collect::<Vec<_>>(),"Entire real page except separately changed global status/footer retains its original viewport");
            assert!(Arc::ptr_eq(
                real.app.remote_scope_source.as_ref().unwrap(),
                &source
            ));
            assert!(Arc::ptr_eq(
                real.app
                    .github
                    .as_ref()
                    .unwrap()
                    .scoped_pr_navigation
                    .as_ref()
                    .unwrap(),
                &nav
            ));
            assert!(real
                .app
                .github
                .as_ref()
                .unwrap()
                .scoped_pr_resource
                .is_none());
            intact(&real);
        }
    }
}
#[test]
fn review_threads_v4_detail_fresh_reply_resolve_reopen_use_actual_original_parent_not_fake_summary()
{
    for action in [
        threads::publication_v1::ThreadPublicationActionV1::Reply,
        threads::publication_v1::ThreadPublicationActionV1::Resolve,
        threads::publication_v1::ThreadPublicationActionV1::Reopen,
    ] {
        let resolved = action == threads::publication_v1::ThreadPublicationActionV1::Reopen;
        let mut browse = thread_steps(Some("LINE"));
        let mut raw = thread_page(Some("LINE"), false);
        let node = &mut raw["data"]["node"]["reviewThreads"]["nodes"][0];
        node["isResolved"] = json!(resolved);
        node["viewerCanResolve"] = json!(!resolved);
        node["viewerCanUnresolve"] = json!(resolved);
        browse[9] = graph(raw);
        browse.extend(observation_steps(selected_node(resolved, true)));
        browse.extend(observation_steps(selected_node(resolved, true)));
        let mut real = detail_real(true, browse);
        native_v1::open_detail_v4(&mut real.app);
        let mut bg = Background::new();
        thread_finish(&mut real, &mut bg);
        let intent =
            threads::publication_v1::ScopedThreadPublicationIntentV1::capture(&real.app, action)
                .unwrap();
        let draft = threads::publication_v1::ScopedThreadDraftV1::open(
            intent.clone(),
            &provider(&real.fixture),
            &database(&real.fixture),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let mut form = threads::publication_v1::ui_v1::form_from_draft(draft).unwrap();
        let body: Arc<str> = if action == threads::publication_v1::ThreadPublicationActionV1::Reply
        {
            Arc::from("Exact Timeline fresh reply body 界")
        } else {
            Arc::from("")
        };
        if !body.is_empty() {
            threads::publication_v1::ui_v1::replace_editor_result(&mut form, &body).unwrap();
        }
        let request = threads::publication_v1::ui_v1::request(&form).unwrap();
        let preview = threads::publication_v1::ScopedThreadPreviewV1::prepare(
            intent,
            body,
            &provider(&real.fixture),
            &database(&real.fixture),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(preview.identity().account_id, Some(42));
        assert!(preview
            .lines()
            .iter()
            .any(|line| line.contains("GENUINE_PR_NODE_1007") || line.contains("THREAD_ACTUAL_9")));
        assert!(threads::publication_v1::ui_v1::accept_preview(
            &mut form, &request, preview
        ));
        assert!(form.preview);
        assert!(form.plan_ready());
        intact(&real);
    }
}
#[test]
fn review_threads_v4_detail_capture_is_scalar_original_source_tab_selection_scroll_and_numeric_scope_bound(
) {
    let mut real = detail_real(true, vec![]);
    let intent = threads::ThreadBrowseIntentV1::capture_detail_v4(&real.app).unwrap();
    assert!(intent.owns(&real.app));
    assert_eq!(intent.path(), None);
    assert!(intent.selected_path_v4().is_none());
    real.app.inbox_list.top += 1;
    assert!(!intent.owns(&real.app));
    real.app.inbox_list.top -= 1;
    real.app.inbox_list.selected += 1;
    assert!(!intent.owns(&real.app));
    real.app.inbox_list.selected -= 1;
    let old = real.app.github.as_ref().unwrap().identity.account_id;
    real.app.github.as_mut().unwrap().identity.account_id = Some(43);
    assert!(!intent.owns(&real.app));
    real.app.github.as_mut().unwrap().identity.account_id = old;
    real.app
        .github
        .as_mut()
        .unwrap()
        .pr
        .as_mut()
        .unwrap()
        .details
        .summary
        .base_sha = "f".repeat(40);
    assert!(!intent.owns(&real.app));
    assert!(threads::ThreadBrowseIntentV1::capture_detail_v4(&real.app).is_err());
    intact(&real);
}
#[test]
fn review_threads_v4_detail_actual_account_or_recreated_repository_failure_hides_private_browser_before_effects(
) {
    for wrong_uid in [false, true] {
        let steps = if wrong_uid {
            principal(43, "same-login-octocat")
        } else {
            let mut rows = principal(42, "octocat");
            rows.push(get(&format!("repos/{REPO}"), repo(9999, REPO)));
            rows
        };
        let mut real = detail_real(true, steps);
        native_v1::open_detail_v4(&mut real.app);
        let mut bg = Background::new();
        thread_finish(&mut real, &mut bg);
        assert!(real.app.scoped_threads_v1.is_none());
        assert!(!paint(&mut real.app).contains(BODY));
        assert!(real
            .app
            .github
            .as_ref()
            .is_none_or(|github| github.pr.is_none()));
        intact(&real);
    }
}
