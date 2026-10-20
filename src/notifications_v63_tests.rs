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
                unread: row.unread,
            })
            .collect(),
        detail: actual.detail.clone(),
        coverage: actual.coverage.clone(),
        attempts: actual.attempts.clone(),
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
pub(crate) fn steps(final_uid: u64) -> Vec<GhStep> {
    let mut repository = support::row(0);
    repository["url"] = json!("https://github.invalid/api/v3/repos/example/repo0000");
    let row = json!({"id":"9007199254740993","unread":true,"reason":"review_requested","updated_at":"2026-10-15T18:00:00Z","last_read_at":null,
        "url":"https://github.invalid/api/v3/notifications/threads/9007199254740993","subscription_url":"https://github.invalid/api/v3/notifications/threads/9007199254740993/subscription",
        "repository":repository,"subject":{"title":"PRIVATE_NOTIFICATION_ACTUAL_SOURCE 界","url":"https://github.invalid/api/v3/repos/example/repo0000/pulls/7","latest_comment_url":"https://github.invalid/api/v3/repos/example/repo0000/issues/comments/99","type":"PullRequest"}});
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
    for _ in 0..2 {
        steps.push(support::get("repos/example/repo0000", support::row(0)));
    }
    steps.extend(support::principal(final_uid));
    steps
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
                .contains("bulk PUT is not an inverse"));
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
