//! Genuine original provider/SQL plus real Background admission. Cached Inbox
//! principal is input intent only; all private rows come from actual children.
use super::*;
use crate::{
    notifications_ui_v63 as ui, repository_catalogue_read_limits_test_support_v1 as support,
    test_support::{Fixture, GhStep},
};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::json;
#[test]
fn notifications_native_v63_genuine_read_cannot_impersonate_follow_closed_result_arm() {
    let f = Fixture::new();
    let expected = steps(42);
    f.scripted_gh(&expected);
    let mut app = app();
    let load = captured(&mut app);
    let result = execute(load.clone(), &support::provider(&f), &support::database(&f));
    crate::notifications_native_v63::publish(&mut app, &load, result);
    let view = app.notifications_v63.as_ref().unwrap();
    let actual = view.prepared.as_ref().unwrap();
    // Retain the ACTUAL closed source and bounded display Arcs, not a proof,
    // owner or parent constructor. This is deliberate wrong-kind poison, not
    // claimed actual worker failure/panic coverage.
    let wrong_read = PreparedNotificationsV63 {
        source: actual.source.clone(),
        rows: actual
            .rows
            .iter()
            .map(|row| PreparedNotificationRowV63 {
                label: row.label.clone(),
                lead: row.lead,
                actor: row.actor.clone(),
                unread: row.unread,
            })
            .collect(),
        detail: actual.detail.clone(),
        coverage: actual.coverage.clone(),
        attempts: actual.attempts.clone(),
        unchanged: actual.unchanged,
    };
    app.dispatch(crate::app::Action::NotificationsV63(
        ui::NotificationActionV63::FollowPr,
    ));
    let follow = app
        .notifications_work_v63
        .take()
        .expect("genuine current explicit typed Follow");
    assert!(follow.matches(&app));
    let before = paint(&mut app, 120, 40);
    let wrong = PreparedNotificationResultV63 {
        original: follow.clone(),
        result: Ok(PreparedNotificationWorkV63::Read(wrong_read)),
        hide_original_private: false,
    };
    assert!(!wrong.matches(&follow));
    assert_eq!(paint(&mut app, 120, 40), before);
    assert!(app.notification_pr_source_v63.is_none());
    assert!(app.notifications_v63.as_ref().unwrap().prepared.is_some());
    support::exact(&f, &expected);
}
fn identity(uid: u64) -> HostIdentity {
    HostIdentity {
        host: support::HOST.into(),
        login: "intent-octocat".into(),
        account_id: Some(uid),
        gh_version: String::new(),
    }
}
/// The one fixture notification thread, as the list and thread GETs return it.
fn thread_value(unread: bool) -> serde_json::Value {
    let mut repository = support::row(0);
    repository["url"] = json!("https://github.invalid/api/v3/repos/example/repo0000");
    json!({"id":"9007199254740993","unread":unread,"reason":"review_requested","updated_at":"2026-10-15T18:00:00Z",
        "last_read_at":if unread { serde_json::Value::Null } else { json!("2026-10-15T18:01:00Z") },
        "url":"https://github.invalid/api/v3/notifications/threads/9007199254740993","subscription_url":"https://github.invalid/api/v3/notifications/threads/9007199254740993/subscription",
        "repository":repository,"subject":{"title":"PRIVATE_NOTIFICATION_ACTUAL_SOURCE 界","url":"https://github.invalid/api/v3/repos/example/repo0000/pulls/7","latest_comment_url":"https://github.invalid/api/v3/repos/example/repo0000/issues/comments/99","type":"PullRequest"}})
}
pub(crate) fn steps(final_uid: u64) -> Vec<GhStep> {
    let row = thread_value(true);
    let mut steps = support::principal(42);
    steps.push(GhStep::read(
        &[
            "api",
            "notifications?all=false&participating=false&per_page=50&page=1",
            "--hostname",
            support::HOST,
            "--method",
            "GET",
            "--include",
        ],
        &format!(
            "HTTP/2 200 OK\nETag: \"actual\"\r\nX-Poll-Interval: 60\r\n\r\n{}",
            json!([row])
        ),
    ));
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    steps.push(actors_step());
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    steps.extend(support::principal(final_uid));
    steps
}
/// The list's ONE batched latest-actor read for the single fixture row.
pub(crate) const ACTORS_QUERY: &str = "query{n0:repository(owner:\"example\",name:\"repo0000\"){issueOrPullRequest(number:7){...L}}}fragment L on IssueOrPullRequest{...on Issue{author{login __typename}comments(last:1){nodes{author{login __typename}createdAt}}}...on PullRequest{author{login __typename}comments(last:1){nodes{author{login __typename}createdAt}}reviews(last:1){nodes{author{login __typename}submittedAt}}}}";
pub(crate) fn actors_step_with(response: serde_json::Value) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            support::HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &response.to_string(),
    );
    step.stdin = serde_json::to_vec(&json!({ "query": ACTORS_QUERY })).unwrap();
    step
}
/// The latest issue comment (alice-liddell) is newer than the last review.
pub(crate) fn actors_step() -> GhStep {
    actors_step_with(json!({"data":{"n0":{"issueOrPullRequest":{
        "author":{"login":"pr-author","__typename":"User"},
        "comments":{"nodes":[{"author":{"login":"alice-liddell","__typename":"User"},"createdAt":"2026-10-15T17:00:00Z"}]},
        "reviews":{"nodes":[{"author":{"login":"old-reviewer","__typename":"User"},"submittedAt":"2026-10-14T17:00:00Z"}]}}}}}))
}
pub(crate) fn app() -> App {
    App {
        github: Some(crate::github_workspace::GithubWorkspace::empty(
            identity(42),
            vec![],
            Default::default(),
        )),
        ..Default::default()
    }
}
pub(crate) fn captured(app: &mut App) -> Arc<NotificationWorkV63> {
    app.dispatch(crate::app::Action::NotificationsV63(
        ui::NotificationActionV63::Open,
    ));
    app.notifications_work_v63
        .take()
        .expect("real input captured before queue")
}
fn paint(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}
#[test]
fn notifications_native_v63_genuine_source_watch_respects_server_cooldown_and_stop_zero_more_children(
) {
    let f = Fixture::new();
    let expected = steps(42);
    f.scripted_gh(&expected);
    let mut app = app();
    let work = captured(&mut app);
    let result = execute(work.clone(), &support::provider(&f), &support::database(&f));
    crate::notifications_native_v63::publish(&mut app, &work, result);
    let before = std::fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
    app.dispatch(crate::app::Action::NotificationsV63(
        ui::NotificationActionV63::Watch,
    ));
    assert_eq!(
        app.notifications_v63
            .as_ref()
            .unwrap()
            .watch
            .as_ref()
            .unwrap()
            .remaining(),
        29
    );
    for _ in 0..10 {
        crate::notifications_native_v63::tick(&mut app);
        assert!(app.notifications_work_v63.is_none());
    }
    app.dispatch(crate::app::Action::NotificationsV63(
        ui::NotificationActionV63::Refresh,
    ));
    assert!(app.notifications_work_v63.is_none());
    assert!(paint(&mut app, 80, 24).contains("Watch server cooldown"));
    app.dispatch(crate::app::Action::NotificationsV63(
        ui::NotificationActionV63::Watch,
    ));
    assert!(app.notifications_v63.as_ref().unwrap().watch.is_none());
    assert_eq!(
        std::fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before
    );
    support::exact(&f, &expected);
}
#[test]
fn notifications_native_v63_failed_batched_actor_read_keeps_rows_and_says_unknown() {
    let f = Fixture::new();
    let mut expected = steps(42);
    assert!(support::graphql_read(&expected[4]));
    expected[4] = actors_step_with(json!({"errors":[{"message":"rate limited"}]}));
    f.scripted_gh(&expected);
    let mut app = app();
    let work = captured(&mut app);
    let result = execute(work.clone(), &support::provider(&f), &support::database(&f));
    crate::notifications_native_v63::publish(&mut app, &work, result);
    let prepared = app
        .notifications_v63
        .as_ref()
        .unwrap()
        .prepared
        .as_ref()
        .unwrap();
    assert!(prepared.rows[0].actor.is_none());
    assert!(
        prepared
            .coverage
            .contains("Complete query | actors unavailable | 1 raw"),
        "{}",
        prepared.coverage
    );
    let frame = paint(&mut app, 120, 40);
    assert!(
        frame.contains("Review requested • PR #7 • — • PRIVATE_NOTIFICATION_ACTUAL_SOURCE"),
        "{frame}"
    );
    assert!(!frame.contains("pr-author") && !frame.contains("alice-liddell"));
    support::exact(&f, &expected);
}
#[test]
fn notifications_native_v63_actual_read_prepared_display_and_resize_do_zero_more_io() {
    let f = Fixture::new();
    let expected = steps(42);
    f.scripted_gh(&expected);
    let mut app = app();
    let work = captured(&mut app);
    let result = execute(work.clone(), &support::provider(&f), &support::database(&f));
    crate::notifications_native_v63::publish(&mut app, &work, result);
    assert!(app.notifications_v63.as_ref().unwrap().prepared.is_some());
    assert!(app
        .notifications_v63
        .as_ref()
        .unwrap()
        .prepared
        .as_ref()
        .unwrap()
        .coverage
        .contains("1 raw, 0 duplicates, 0 unverified repo"));
    let label = app
        .notifications_v63
        .as_ref()
        .unwrap()
        .prepared
        .as_ref()
        .unwrap()
        .rows[0]
        .label
        .clone();
    assert!(label.starts_with("Unread "), "{label}");
    assert!(
        label.contains(" Review requested • PR #7 • PRIVATE_NOTIFICATION_ACTUAL_SOURCE 界 • "),
        "{label}"
    );
    assert!(label.ends_with(" • example/repo0000"), "{label}");
    // The batched read named the latest commenter (newer than the review),
    // never the PR author; the row draws its chip before the title.
    let row = &app
        .notifications_v63
        .as_ref()
        .unwrap()
        .prepared
        .as_ref()
        .unwrap()
        .rows[0];
    let actor = row
        .actor
        .as_ref()
        .expect("latest actor from the batched read");
    assert_eq!(actor.login.as_deref(), Some("alice-liddell"));
    let chip = crate::identity::InitialsChip::text(actor, app.theme().mode);
    let wide = paint(&mut app, 120, 40);
    assert!(
        wide.contains(&format!(
            "Review requested • PR #7 • {chip} alice-liddell • PRIVATE_NOTIFICATION_ACTUAL_SOURCE"
        )),
        "{wide}"
    );
    let narrow = paint(&mut app, 80, 24);
    assert!(
        narrow.contains(&format!(
            "Review requested • PR #7 • {chip} PRIVATE_NOTIFICATION_ACTUAL_SOURCE"
        )),
        "{narrow}"
    );
    let before = std::fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
    for (w, h) in [(80, 24), (120, 40), (40, 18), (120, 40)] {
        let frame = paint(&mut app, w, h);
        assert!(frame.contains("Notifications"));
        assert!(frame.contains("[b Back / Cancel]"));
        if w >= 80 {
            assert!(frame.contains("PRIVATE_NOTIFICATION_ACTUAL_SOURCE"));
            assert!(!frame.contains("Some("));
            assert!(!frame.contains("None"));
        }
    }
    assert_eq!(
        std::fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before
    );
    support::exact(&f, &expected);
}

#[test]
fn notifications_native_v63_actual_focus_is_unique_for_tab_backtab_enter_and_mouse_at_both_widths()
{
    use crate::scoped_actions_test_support_v1 as git;
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::style::Modifier;
    use rusqlite::{types::Value as SqlValue, Connection, OpenFlags};
    use std::collections::BTreeMap;

    fn sql(f: &Fixture) -> BTreeMap<String, Vec<Vec<String>>> {
        let connection =
            Connection::open_with_flags(support::database(f), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        let names = connection
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        let mut result = BTreeMap::new();
        for name in names {
            assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
            let mut statement = connection
                .prepare(&format!("SELECT * FROM \"{name}\""))
                .unwrap();
            let columns = statement.column_count();
            let mut rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|index| row.get::<_, SqlValue>(index).map(|v| format!("{v:?}")))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            rows.sort();
            result.insert(name, rows);
        }
        result
    }

    for no_color in [false, true] {
        for (width, height) in [(80, 24), (120, 40)] {
            let f = Fixture::new();
            git::git(&f, &["init", "--initial-branch=main"]);
            std::fs::write(f.repo().join("tracked"), b"ORIGINAL_COMMITTED\n").unwrap();
            git::git(&f, &["add", "tracked"]);
            git::git(
                &f,
                &[
                    "-c",
                    "user.name=Actual fixture author",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "commit",
                    "-m",
                    "actual original Git state",
                ],
            );
            std::fs::write(f.repo().join("tracked"), b"UNCHANGED_DIRTY_WORKTREE\n").unwrap();
            let expected = steps(42);
            f.scripted_gh(&expected);
            let mut app = app();
            app.no_color = no_color;
            let work = captured(&mut app);
            let result = execute(work.clone(), &support::provider(&f), &support::database(&f));
            crate::notifications_native_v63::publish(&mut app, &work, result);
            let source = app
                .notifications_v63
                .as_ref()
                .unwrap()
                .prepared
                .as_ref()
                .unwrap()
                .source
                .clone();
            let before_sql = sql(&f);
            let before_git = git::git_state(&f);
            let before_gh = std::fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
            let before_status = app.status.clone();
            let before_loading = app.loading;
            let check_focus = |app: &mut App, expected: Option<ui::NotificationActionV63>| {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
                let view = app.notifications_v63.as_ref().unwrap();
                assert!(Arc::ptr_eq(
                    &view.prepared.as_ref().unwrap().source,
                    &source
                ));
                let controls = view
                    .hits
                    .iter()
                    .take_while(|(_, action)| {
                        !matches!(action, ui::NotificationActionV63::Select(_))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(controls.len(), 13);
                let inverse = controls
                    .iter()
                    .filter(|(rect, _)| {
                        terminal.backend().buffer()[(rect.x, rect.y)]
                            .modifier
                            .contains(Modifier::REVERSED)
                    })
                    .map(|(_, action)| *action)
                    .collect::<Vec<_>>();
                assert_eq!(inverse, expected.into_iter().collect::<Vec<_>>());
                // The genuinely selected body row is inverse too, but cannot
                // masquerade as any declared control's keyboard focus.
                assert!(view.hits.iter().any(|(rect, action)| {
                    matches!(action, ui::NotificationActionV63::Select(_))
                        && terminal.backend().buffer()[(rect.x, rect.y)]
                            .modifier
                            .contains(Modifier::REVERSED)
                }));
                assert_eq!(app.hits.len(), view.hits.len());
            };
            let key = |app: &mut App, code| {
                app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
            };
            check_focus(&mut app, None);
            key(&mut app, KeyCode::Tab);
            check_focus(&mut app, Some(ui::NotificationActionV63::Back));
            key(&mut app, KeyCode::Tab);
            check_focus(&mut app, Some(ui::NotificationActionV63::Quit));
            key(&mut app, KeyCode::BackTab);
            check_focus(&mut app, Some(ui::NotificationActionV63::Back));
            key(&mut app, KeyCode::BackTab);
            check_focus(&mut app, Some(ui::NotificationActionV63::MarkUnread));
            key(&mut app, KeyCode::Enter);
            assert!(app
                .notifications_v63
                .as_ref()
                .unwrap()
                .message
                .contains("GitHub has no API to mark a thread unread"));
            assert!(app.notifications_work_v63.is_none());
            assert!(app.notifications_owner_v63.is_none());
            check_focus(&mut app, Some(ui::NotificationActionV63::MarkUnread));
            let target = app
                .hits
                .iter()
                .find(|hit| {
                    hit.action
                        == crate::app::Action::NotificationsV63(
                            ui::NotificationActionV63::MarkUnread,
                        )
                })
                .expect("actual same disabled-action hit from complete production draw")
                .rect;
            let message = app.notifications_v63.as_ref().unwrap().message;
            app.handle_event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: target.x,
                row: target.y,
                modifiers: KeyModifiers::NONE,
            }));
            assert_eq!(app.notifications_v63.as_ref().unwrap().message, message);
            check_focus(&mut app, Some(ui::NotificationActionV63::MarkUnread));
            assert!(app.notifications_work_v63.is_none());
            assert!(app.notifications_owner_v63.is_none());
            assert_eq!(app.status, before_status);
            assert_eq!(app.loading, before_loading);
            assert_eq!(sql(&f), before_sql);
            assert_eq!(git::git_state(&f), before_git);
            assert_eq!(
                std::fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
                before_gh
            );
            support::exact(&f, &expected);
        }
    }
}
#[test]
fn notifications_native_v63_actual_final_account_loss_never_publishes_body() {
    let f = Fixture::new();
    let expected = steps(43);
    f.scripted_gh(&expected);
    let mut app = app();
    let work = captured(&mut app);
    let result = execute(work.clone(), &support::provider(&f), &support::database(&f));
    crate::notifications_native_v63::publish(&mut app, &work, result);
    let frame = paint(&mut app, 120, 40);
    assert!(!frame.contains("PRIVATE_NOTIFICATION_ACTUAL_SOURCE"));
    assert!(app.notifications_v63.is_none());
    assert!(app.github.is_none());
    support::exact(&f, &expected);
}
#[test]
fn notifications_native_v63_idle_current_account_loss_hides_exact_inbox_fallback_without_touching_successor_slot_status(
) {
    let f = Fixture::new();
    let expected = steps(42);
    f.scripted_gh(&expected);
    let mut app = app();
    let work = captured(&mut app);
    let result = execute(work.clone(), &support::provider(&f), &support::database(&f));
    crate::notifications_native_v63::publish(&mut app, &work, result);
    app.content[View::Inbox.index()] = vec!["OLD_PRIVATE_INBOX_FALLBACK".into()];
    app.context = "OLD_PRIVATE_CONTEXT".into();
    app.github.as_mut().unwrap().identity = identity(43);
    app.status = "GENUINE_SUCCESSOR_STATUS".into();
    app.loading[View::Inbox.index()] = true;
    let frame = paint(&mut app, 120, 40);
    assert!(app.github.is_none());
    assert!(app.notifications_v63.is_none());
    assert!(app.content[View::Inbox.index()].is_empty());
    assert!(app.context.is_empty());
    assert!(!frame.contains("PRIVATE_NOTIFICATION_ACTUAL_SOURCE"));
    assert!(!frame.contains("OLD_PRIVATE_INBOX_FALLBACK"));
    assert_eq!(app.status, "GENUINE_SUCCESSOR_STATUS");
    assert!(app.loading[View::Inbox.index()]);
    support::exact(&f, &expected);
}
#[test]
fn notifications_native_v63_independently_newer_page_preserved_and_old_private_rows_never_drawn() {
    let f = Fixture::new();
    let expected = steps(42);
    f.scripted_gh(&expected);
    let mut app = app();
    let work = captured(&mut app);
    let result = execute(work.clone(), &support::provider(&f), &support::database(&f));
    crate::notifications_native_v63::publish(&mut app, &work, result);
    let mut newer =
        crate::github_workspace::GithubWorkspace::empty(identity(42), vec![], Default::default());
    newer.generation = 99;
    app.github = Some(newer);
    app.context = "NEWER_REAL_PAGE_CONTEXT".into();
    app.status = "NEWER_REAL_STATUS".into();
    app.loading[View::Inbox.index()] = true;
    let frame = paint(&mut app, 120, 40);
    assert!(!frame.contains("PRIVATE_NOTIFICATION_ACTUAL_SOURCE"));
    assert_eq!(app.github.as_ref().unwrap().generation, 99);
    assert_eq!(app.context, "NEWER_REAL_PAGE_CONTEXT");
    assert_eq!(app.status, "NEWER_REAL_STATUS");
    assert!(app.loading[View::Inbox.index()]);
    support::exact(&f, &expected);
}
#[test]
fn notifications_native_v63_actual_background_same_inbox_admission_and_current_owner_publication() {
    let f = Fixture::new();
    let expected = steps(42);
    f.scripted_gh(&expected);
    let mut app = app();
    let work = captured(&mut app);
    let token = work.cancellation().clone();
    let mut background = crate::runtime::Background::new();
    assert!(background.submit_notifications_v63(
        &mut app,
        work.clone(),
        support::provider(&f),
        support::database(&f)
    ));
    assert!(app
        .notifications_owner_v63
        .as_ref()
        .is_some_and(|owner| Arc::ptr_eq(owner, &work)));
    assert!(app.loading[View::Inbox.index()]);
    let end = Instant::now() + Duration::from_secs(8);
    while app.loading[View::Inbox.index()] {
        background.poll(&mut app);
        assert!(
            Instant::now() < end,
            "actual notification Background did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!token.is_cancelled());
    assert!(app.notifications_owner_v63.is_none());
    assert!(app.notifications_v63.as_ref().unwrap().prepared.is_some());
    assert!(paint(&mut app, 120, 40).contains("PRIVATE_NOTIFICATION_ACTUAL_SOURCE"));
    support::exact(&f, &expected);
}
#[test]
fn notifications_native_v63_busy_and_read_only_history_controls_do_not_publish_or_bypass_current_input(
) {
    let mut app = app();
    let work = captured(&mut app);
    app.notifications_owner_v63 = Some(work.clone());
    let revision = app.notifications_v63.as_ref().unwrap().revision;
    for action in [
        ui::NotificationActionV63::Details,
        ui::NotificationActionV63::History,
        ui::NotificationActionV63::Refresh,
        ui::NotificationActionV63::IncludeRead,
        ui::NotificationActionV63::Participating,
    ] {
        ui::dispatch(&mut app, action);
    }
    assert!(app.notifications_work_v63.is_none());
    assert_eq!(app.notifications_v63.as_ref().unwrap().revision, revision);
    assert!(app.form.is_none());
    assert!(Arc::ptr_eq(
        app.notifications_owner_v63.as_ref().unwrap(),
        &work
    ));
    ui::dispatch(&mut app, ui::NotificationActionV63::Back);
    assert!(app.notifications_v63.is_none());
    assert!(app.notifications_cancel_v63);
    assert!(app.form.is_none());
    assert!(app.github.is_some());
}

#[test]
fn notifications_native_v63_actual_background_caught_refresh_panic_retires_busy_without_old_private_rows(
) {
    use std::sync::atomic::Ordering;
    let f = Fixture::new();
    let expected = steps(42);
    f.scripted_gh(&expected);
    let provider = support::provider(&f);
    let mut app = app();
    let first = captured(&mut app);
    let result = execute(first.clone(), &provider, &support::database(&f));
    crate::notifications_native_v63::publish(&mut app, &first, result);
    assert!(app.notifications_v63.as_ref().unwrap().prepared.is_some());
    ui::dispatch(&mut app, ui::NotificationActionV63::Refresh);
    let work = app.notifications_work_v63.take().unwrap();
    assert!(work.previous.is_some());
    assert_eq!(work.phase, NotificationReadPhaseV63::Refresh);
    work.actual_worker_fault_v63.store(1, Ordering::Release);
    let mut background = crate::runtime::Background::new();
    assert!(background.submit_notifications_v63(&mut app, work, provider, support::database(&f)));
    let end = Instant::now() + Duration::from_secs(8);
    while app.loading[View::Inbox.index()] {
        background.poll(&mut app);
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.notifications_owner_v63.is_none());
    assert!(app.notifications_v63.as_ref().unwrap().prepared.is_none());
    let frame = paint(&mut app, 120, 40);
    assert!(!frame.contains("PRIVATE_NOTIFICATION_ACTUAL_SOURCE"));
    assert!(app.status.contains("failed"));
    support::exact(&f, &expected);
}
#[test]
fn notifications_native_v63_actual_background_genuine_result_mixed_poison_rejected_before_all_effects(
) {
    use std::sync::atomic::Ordering;
    let f = Fixture::new();
    let expected = steps(42);
    f.scripted_gh(&expected);
    let mut app = app();
    let work = captured(&mut app);
    work.actual_worker_fault_v63.store(2, Ordering::Release);
    let mut background = crate::runtime::Background::new();
    assert!(background.submit_notifications_v63(
        &mut app,
        work.clone(),
        support::provider(&f),
        support::database(&f)
    ));
    let before_status = app.status.clone();
    let before_context = app.context.clone();
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        background.poll(&mut app);
        if background.notification_completions_observed_v63() == 1 {
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    // Advances ONLY after the SAME real channel completion is received; no
    // paint/wall-clock sleep stands in for actual forbidden-effect refusal.
    assert!(app.loading[View::Inbox.index()]);
    assert!(app
        .notifications_owner_v63
        .as_ref()
        .is_some_and(|owner| Arc::ptr_eq(owner, &work)));
    assert_eq!(app.status, before_status);
    assert_eq!(app.context, before_context);
    assert!(app.notifications_v63.as_ref().unwrap().prepared.is_none());
    assert!(!paint(&mut app, 120, 40).contains("MIXED FACET"));
    assert!(background.cancel_notifications_v63(&mut app));
    assert!(!app.loading[View::Inbox.index()]);
    support::exact(&f, &expected);
}

#[test]
fn notifications_native_v63_row_label_reads_state_age_reason_title_and_resource() {
    // 2026-10-15T18:00:00Z is 1_792_087_200; five days and one hour later.
    let now = 1_792_087_200 + 5 * 86_400 + 3_600;
    let label = row_label_v63(
        true,
        "2026-10-15T18:00:00Z",
        now,
        "review_requested",
        "Fix the parser",
        "example/repo",
        "PR #7",
    );
    assert_eq!(
        label,
        "Unread   5d Review requested • PR #7 • Fix the parser • example/repo"
    );
    let read = row_label_v63(
        false,
        "2026-10-20T18:59:00Z",
        now,
        "mention",
        "t",
        "o/r",
        "Issue #3",
    );
    assert_eq!(read, "Read     1m Mentioned • Issue #3 • t • o/r");
    // An unparseable time never invents an age.
    let unknown = row_label_v63(
        false,
        "yesterday",
        now,
        "comment",
        "t",
        "o/r",
        "Commit abc1234",
    );
    assert_eq!(unknown, "Read      — Comment • Commit abc1234 • t • o/r");
}

#[test]
fn notifications_native_v63_reason_labels_are_readable_and_unknown_reasons_stay_observed() {
    for (raw, readable) in [
        ("review_requested", "Review requested"),
        ("mention", "Mentioned"),
        ("team_mention", "Team mentioned"),
        ("assign", "Assigned"),
        ("ci_activity", "CI activity"),
        ("state_change", "State changed"),
        ("subscribed", "Watching"),
        ("manual", "Subscribed manually"),
        ("security_alert", "Security alert"),
    ] {
        assert_eq!(reason_label_v63(raw), readable);
    }
    assert_eq!(reason_label_v63("brand_new_reason"), "brand_new_reason");
    // Control bytes in an unknown reason are sanitized and the label bounded.
    let hostile = format!("x\u{1b}[31m{}", "y".repeat(100));
    let label = reason_label_v63(&hostile);
    assert!(!label.contains('\u{1b}'));
    assert!(label.chars().count() <= 40);
}

fn details_steps(actor: GhStep) -> Vec<GhStep> {
    let mut steps = support::principal(42);
    let mut page = GhStep::read(
        &[
            "api",
            "notifications?all=false&participating=false&per_page=50&page=1",
            "--hostname",
            support::HOST,
            "--method",
            "GET",
            "--include",
            "--header",
            "If-None-Match: \"actual\"",
        ],
        "HTTP/2 304 Not Modified\nETag: \"actual\"\r\nContent-Length: 17\r\nX-Poll-Interval: 60\r\n\r\n",
    );
    page.status = 1;
    page.stderr = b"gh: HTTP 304\n".to_vec();
    steps.push(page);
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    steps.push(actors_step());
    steps.push(actor);
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    steps.extend(support::principal(42));
    steps
}
fn details_frame(actor: GhStep) -> String {
    let f = Fixture::new();
    let mut expected = steps(42);
    expected.extend(details_steps(actor));
    f.scripted_gh(&expected);
    let mut app = app();
    // ONE issuer: a refresh against a different runner is refused by design.
    let provider = support::provider(&f);
    let load = captured(&mut app);
    let result = execute(load.clone(), &provider, &support::database(&f));
    crate::notifications_native_v63::publish(&mut app, &load, result);
    ui::dispatch(&mut app, ui::NotificationActionV63::Details);
    let work = app
        .notifications_work_v63
        .take()
        .expect("actual Details read captured before queue");
    assert_eq!(work.phase, NotificationReadPhaseV63::Details);
    let result = execute(work.clone(), &provider, &support::database(&f));
    crate::notifications_native_v63::publish(&mut app, &work, result);
    let frame = paint(&mut app, 120, 40);
    // Exactly ONE latest-comment child, between the two repository checks.
    support::exact(&f, &expected);
    frame
}
#[test]
fn notifications_native_v63_details_read_latest_commenter_once_with_initials_chip() {
    let frame = details_frame(support::get(
        "repos/example/repo0000/issues/comments/99",
        json!({"id":99,"user":{"login":"alice-liddell","id":5,"type":"User"}}),
    ));
    assert!(
        frame.contains("Latest activity: [AL] @alice-liddell • latest comment"),
        "{frame}"
    );
    assert!(
        frame.contains("PRIVATE_NOTIFICATION_ACTUAL_SOURCE"),
        "{frame}"
    );
}
#[test]
fn notifications_native_v63_details_unreadable_latest_comment_stays_unknown_not_guessed() {
    let mut missing = support::get(
        "repos/example/repo0000/issues/comments/99",
        json!({"message":"Not Found"}),
    );
    missing.status = 1;
    missing.stderr = b"gh: Not Found (HTTP 404)\n".to_vec();
    let frame = details_frame(missing);
    assert!(
        frame.contains(
            "Latest activity: Acting user unavailable • latest comment could not be read"
        ),
        "{frame}"
    );
}
#[test]
fn notifications_native_v63_latest_actor_line_labels_subject_author_and_unknown_reason() {
    use crate::providers::gh::{Actor, NotificationLatestActorV63 as Latest};
    let author = Latest::Observed {
        actor: Actor {
            login: "bob".into(),
            display_name: None,
            avatar_url: None,
            actor_type: "User".into(),
        },
        is_subject: true,
    };
    assert_eq!(
        latest_actor_line_v63("github.com", &author, "Acting user unavailable"),
        "Latest activity: [BO] @bob • subject author (no newer comment)"
    );
    let bot = Latest::Observed {
        actor: Actor {
            login: "dependabot[bot]".into(),
            display_name: None,
            avatar_url: None,
            actor_type: "Bot".into(),
        },
        is_subject: false,
    };
    assert!(latest_actor_line_v63("github.com", &bot, "x").contains("@dependabot[bot] [bot]"));
    assert_eq!(
        latest_actor_line_v63(
            "github.com",
            &Latest::Unavailable("no latest comment observed"),
            "Acting user unavailable"
        ),
        "Latest activity: Acting user unavailable • no latest comment observed"
    );
}

const THREAD: &str = "9007199254740993";
fn subscription_value(ignored: bool) -> serde_json::Value {
    json!({"subscribed":!ignored,"ignored":ignored,"reason":null,"created_at":"2026-10-15T18:00:00Z",
        "url":format!("https://github.invalid/api/v3/notifications/threads/{THREAD}/subscription"),
        "thread_url":format!("https://github.invalid/api/v3/notifications/threads/{THREAD}")})
}
/// The Preview's (and again the Confirm's) fresh selected-thread reads.
fn write_pre(ignore: bool) -> Vec<GhStep> {
    let mut steps = support::principal(42);
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    for _ in 0..2 {
        steps.push(support::get(
            &format!("notifications/threads/{THREAD}"),
            thread_value(true),
        ));
        if ignore {
            steps.push(support::get(
                &format!("notifications/threads/{THREAD}/subscription"),
                subscription_value(false),
            ));
        }
    }
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    steps.extend(support::principal(42));
    steps
}
/// ONE write, its exact readback and the closing account/repository checks.
fn write_post(ignore: bool) -> Vec<GhStep> {
    let endpoint = if ignore {
        format!("notifications/threads/{THREAD}/subscription")
    } else {
        format!("notifications/threads/{THREAD}")
    };
    let mut args = vec![
        "api",
        endpoint.as_str(),
        "--hostname",
        support::HOST,
        "--method",
        if ignore { "PUT" } else { "PATCH" },
        "--include",
    ];
    let raw = if ignore {
        args.extend(["--input", "-"]);
        format!("HTTP/2.0 200 OK\n\r\n{}", subscription_value(true))
    } else {
        "HTTP/2.0 205 Reset Content\nContent-Length: 0\r\n\r\n".into()
    };
    let mut write = GhStep::read(&args, &raw);
    if ignore {
        write.stdin = serde_json::to_vec(&json!({"ignored":true})).unwrap();
    }
    let mut repo = support::row(0);
    repo["url"] = json!("https://github.invalid/api/v3/repos/example/repo0000");
    vec![
        write,
        support::get(
            &endpoint,
            if ignore {
                subscription_value(true)
            } else {
                thread_value(false)
            },
        ),
        support::get("user", json!({"id":42,"login":"actual"})),
        support::get("repos/example/repo0000", repo),
        support::get("user", json!({"id":42,"login":"actual"})),
    ]
}
/// Every child matched in order with its exact argv and stdin, writes included.
fn exact_mixed(f: &Fixture, steps: &[GhStep]) -> usize {
    let rows = std::fs::read_to_string(f.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), steps.len(), "no missing or additional gh child");
    let mut writes = 0;
    for (index, (actual, expected)) in rows.iter().zip(steps).enumerate() {
        assert_eq!(actual["step"], index);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        let hex = expected
            .stdin
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(actual["stdin_hex"], hex);
        if !support::graphql_read(expected)
            && expected
                .args
                .iter()
                .any(|arg| matches!(arg.as_str(), "PATCH" | "PUT" | "POST" | "DELETE"))
        {
            writes += 1;
        }
    }
    writes
}
fn run(app: &mut App, provider: &GithubProvider, f: &Fixture) {
    let work = app
        .notifications_work_v63
        .take()
        .expect("one explicit phase captured before queue");
    let result = execute(work.clone(), provider, &support::database(f));
    crate::notifications_native_v63::publish(app, &work, result);
}
fn confirmed_write_rereads_queue(ignore: bool) {
    let f = Fixture::new();
    let mut expected = steps(42);
    expected.extend(write_pre(ignore));
    expected.extend(write_pre(ignore));
    expected.extend(write_post(ignore));
    // The automatic re-read of the SAME query: Mark read leaves the
    // Unread-only queue empty; Ignore keeps the (still unread) thread.
    if ignore {
        expected.extend(steps(42));
    } else {
        expected.extend(support::principal(42));
        expected.push(GhStep::read(
            &[
                "api",
                "notifications?all=false&participating=false&per_page=50&page=1",
                "--hostname",
                support::HOST,
                "--method",
                "GET",
                "--include",
            ],
            "HTTP/2 200 OK\nETag: \"after\"\r\nX-Poll-Interval: 60\r\n\r\n[]",
        ));
        expected.extend(support::principal(42));
    }
    f.scripted_gh(&expected);
    let provider = support::provider(&f);
    let mut app = app();
    let load = captured(&mut app);
    app.notifications_work_v63 = Some(load);
    run(&mut app, &provider, &f);
    let before = paint(&mut app, 120, 40);
    assert!(
        before.contains("1 observed | Complete query | 1 raw"),
        "{before}"
    );
    ui::dispatch(
        &mut app,
        if ignore {
            ui::NotificationActionV63::Ignore
        } else {
            ui::NotificationActionV63::MarkRead
        },
    );
    run(&mut app, &provider, &f);
    // The Preview body must be painted before Confirm draws enabled: the
    // first frame paints the Preview, the second draws Confirm enabled.
    paint(&mut app, 120, 40);
    paint(&mut app, 120, 40);
    ui::dispatch(&mut app, ui::NotificationActionV63::Confirm);
    assert_eq!(
        app.notifications_work_v63.as_ref().unwrap().phase,
        NotificationReadPhaseV63::Publish
    );
    run(&mut app, &provider, &f);
    // Confirmed: the queue stays open on the same account and ONE fresh
    // (unconditional) read of the SAME query is queued.
    assert_eq!(app.overlay, Some(crate::app::Overlay::NotificationsV63));
    assert!(app.github.is_some());
    let reread = app
        .notifications_work_v63
        .as_ref()
        .expect("confirmed write re-reads its queue");
    assert_eq!(reread.phase, NotificationReadPhaseV63::Load);
    assert!(reread.previous.is_none());
    assert!(reread.query == NotificationQueryV63::default());
    run(&mut app, &provider, &f);
    let after = paint(&mut app, 120, 40);
    if ignore {
        assert!(
            after.contains("Thread ignored • this queue was re-read from GitHub"),
            "{after}"
        );
        assert!(
            after.contains("1 observed | Complete query | 1 raw"),
            "{after}"
        );
        assert!(
            after.contains("PRIVATE_NOTIFICATION_ACTUAL_SOURCE"),
            "{after}"
        );
    } else {
        assert!(
            after.contains("Marked read • this queue was re-read from GitHub"),
            "{after}"
        );
        assert!(
            after.contains("0 observed | Complete query | 0 raw"),
            "{after}"
        );
        assert!(after.contains("No notifications observed"), "{after}");
        assert!(
            !after.contains("PRIVATE_NOTIFICATION_ACTUAL_SOURCE"),
            "{after}"
        );
    }
    assert!(app.notifications_work_v63.is_none());
    assert_eq!(exact_mixed(&f, &expected), 1);
    let db = rusqlite::Connection::open(support::database(&f)).unwrap();
    let (action, state): (String, String) = db
        .query_row(
            "SELECT action,state FROM notification_write_attempts_v63",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (action.as_str(), state.as_str()),
        (if ignore { "ignore" } else { "mark_read" }, "confirmed")
    );
}
#[test]
fn notifications_native_v63_confirmed_mark_read_rereads_queue_rows_and_counts() {
    confirmed_write_rereads_queue(false);
}
#[test]
fn notifications_native_v63_confirmed_ignore_rereads_queue_rows_and_counts() {
    confirmed_write_rereads_queue(true);
}
#[test]
fn notifications_native_v63_mark_unread_stays_disabled_with_reason_and_no_child() {
    let f = Fixture::new();
    let expected = steps(42);
    f.scripted_gh(&expected);
    let provider = support::provider(&f);
    let mut app = app();
    let load = captured(&mut app);
    app.notifications_work_v63 = Some(load);
    run(&mut app, &provider, &f);
    ui::dispatch(&mut app, ui::NotificationActionV63::MarkUnread);
    assert!(app.notifications_work_v63.is_none());
    let frame = paint(&mut app, 120, 40);
    assert!(
        frame.contains("Mark unread unavailable: GitHub has no API to mark a thread unread"),
        "{frame}"
    );
    support::exact(&f, &expected);
}
/// The 304 a watch poll gets back, then the closing checks.
fn watch_poll_304() -> Vec<GhStep> {
    let mut steps = support::principal(42);
    let mut page = GhStep::read(
        &[
            "api",
            "notifications?all=false&participating=false&per_page=50&page=1",
            "--hostname",
            support::HOST,
            "--method",
            "GET",
            "--include",
            "--header",
            "If-None-Match: \"actual\"",
        ],
        "HTTP/2 304 Not Modified\nETag: \"actual\"\r\nX-Poll-Interval: 120\r\n\r\n",
    );
    page.status = 1;
    page.stderr = b"gh: HTTP 304\n".to_vec();
    steps.push(page);
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    steps.push(actors_step());
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    steps.extend(support::principal(42));
    steps
}
#[test]
fn notifications_native_v63_watch_polls_once_after_server_interval_on_fake_clock_and_keeps_304_rows(
) {
    let f = Fixture::new();
    let mut expected = steps(42);
    expected.extend(watch_poll_304());
    f.scripted_gh(&expected);
    let provider = support::provider(&f);
    let mut app = app();
    // A short configured refresh never beats the server's X-Poll-Interval.
    app.notifications_watch_seconds_v63 = 15;
    let load = captured(&mut app);
    app.notifications_work_v63 = Some(load);
    run(&mut app, &provider, &f);
    ui::dispatch(&mut app, ui::NotificationActionV63::Watch);
    let now = Instant::now();
    // Server interval 60 s: nothing at +59 s (fake clock), however often.
    for _ in 0..5 {
        crate::notifications_native_v63::tick_at(&mut app, now + Duration::from_secs(59));
        assert!(app.notifications_work_v63.is_none());
    }
    crate::notifications_native_v63::tick_at(&mut app, now + Duration::from_secs(61));
    let poll = app
        .notifications_work_v63
        .as_ref()
        .expect("ONE conditional poll after the server interval");
    assert_eq!(poll.phase, NotificationReadPhaseV63::Refresh);
    assert!(poll.previous.is_some());
    // While the poll is outstanding no second poll starts.
    crate::notifications_native_v63::tick_at(&mut app, now + Duration::from_secs(10_000));
    run(&mut app, &provider, &f);
    let frame = paint(&mut app, 120, 40);
    assert!(
        frame.contains("Watch poll: unchanged since last read (HTTP 304) • W Stop"),
        "{frame}"
    );
    assert!(
        frame.contains("PRIVATE_NOTIFICATION_ACTUAL_SOURCE"),
        "{frame}"
    );
    assert!(
        frame.contains("1 observed | Complete query | 1 raw"),
        "{frame}"
    );
    let view = app.notifications_v63.as_ref().unwrap();
    assert_eq!(view.watch.as_ref().unwrap().remaining(), 28);
    // The 304's longer X-Poll-Interval (120 s) now governs.
    let now = Instant::now();
    crate::notifications_native_v63::tick_at(&mut app, now + Duration::from_secs(119));
    assert!(app.notifications_work_v63.is_none());
    support::exact(&f, &expected);
}
