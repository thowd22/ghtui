//! SOURCE ONLY / UNRUN. Actual opaque GH factories, installed Git, SQL and
//! TestBackend geometry. No displayed String/DTO is a log/source capability.
use super::*;
use crate::{
    app::Overlay,
    providers::gh::*,
    scoped_actions_read_v2::LogSearchModeV2 as M,
    scoped_actions_resources_test_support_v2 as r,
    scoped_actions_resources_v2::ScopedActionsReadActionV2 as R,
    scoped_actions_test_support_v1 as f,
    test_support::{Fixture, GhStep},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, Terminal};
use std::time::{Duration, Instant};
const TEXT: &str = "first actual\nerror 界 é\nthird actual\nerror second\nlast\n";
#[test]
fn scoped_actions_read_v2_owned_modal_borrowed_capture_precedes_any_unrelated_event_clone() {
    // Architectural input-bound regression, alongside genuine oversized Paste
    // behavior/SQL/GH/Git tests. This does not manufacture a runtime frame or
    // prove heap/RSS. The external Event cannot be instrumented for Clone.
    let source = include_str!("app.rs");
    let handler = source
        .split_once("pub fn handle_event(&mut self, event: Event)")
        .expect("actual shared App entry")
        .1;
    let first_borrow = handler
        .find("read_ui_v2::event")
        .expect("actual owned borrowed consumer");
    let first_clone = handler
        .find("event.clone()")
        .expect("actual unrelated event clone remains");
    assert!(
        first_borrow < first_clone,
        "Owned editor bound must precede unrelated event copying"
    );
    assert!(
        handler[..first_borrow].contains("self.overlay == Some(Overlay::ScopedActionsReadV2)"),
        "Early capture must be restricted to the actual owned modal, not steal another view"
    );
}
fn logs(uid: u64) -> Vec<GhStep> {
    let mut steps = f::operation(ScopedActionsRequestKindV1::Logs, uid);
    steps
        .iter_mut()
        .find(|step| step.args.first().is_some_and(|a| a == "run"))
        .unwrap()
        .stdout = TEXT.as_bytes().to_vec();
    steps
}
fn loaded(tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut steps = logs(42);
    steps.extend(tail);
    let (fix, p, mut app, all) = f::opened(steps);
    let value = f::read(&mut app, &p, &fix, ActionsAction::Logs(Some(501))).unwrap();
    f::install(&mut app, value);
    assert_eq!(app.actions.as_ref().unwrap().rows.len(), 5);
    (fix, p, app, all)
}
fn ui(app: &mut App, action: read_ui_v2::ReadUiActionV2) {
    assert!(intercept(app, &ActionsAction::ScopedReadV2(R::Ui(action))));
}
fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_event(Event::Key(KeyEvent::new(code, modifiers)));
}
fn paste(app: &mut App, value: &str) {
    app.handle_event(Event::Paste(value.into()));
}
fn render(app: &mut App, width: u16, height: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|frame| {
        let area = frame.area();
        read_ui_v2::draw(frame, app, area);
    })
    .unwrap();
    t.backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}
fn search(
    app: &mut App,
    p: &GithubProvider,
    fix: &Fixture,
    query: &str,
    mode: M,
) -> Result<PreparedScopedActionsV1, String> {
    let state = app.scoped_actions_v1.as_mut().unwrap();
    state.epoch = state.epoch.wrapping_add(1);
    let work = ScopedActionsWorkV1::capture_search_v2(
        state.page.clone(),
        query.into(),
        mode,
        (3, 1),
        state.epoch,
    )?;
    execute(work, p, &f::database(fix))
}
fn unchanged(
    fix: &Fixture,
    sql: &Vec<(String, String, String)>,
    git: &(String, String, Vec<u8>, Vec<u8>, Vec<u8>),
) {
    assert_eq!(f::sql_state(fix), *sql);
    assert_eq!(f::git_state(fix), *git);
}
#[test]
fn scoped_actions_read_v2_filters_all_fields_request_one_actual_encoded_page() {
    let query=format!("repos/{}/actions/workflows/7/runs?per_page=100&actor=actual-ci-actor&branch=main&event=push&status=failure&head_sha={}&created=2026-10-15&page=1",f::REPO,"a".repeat(40));
    let mut op = r::page(1, None, 9000, vec![f::run()]);
    op[3] = f::get(
        &query,
        serde_json::json!({"total_count":9000,"workflow_runs":[f::run()]}),
    );
    let (fix, p, mut app, all) = f::setup(op);
    let sql = f::sql_state(&fix);
    let git = f::git_state(&fix);
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenFilters);
    assert!(render(&mut app, 100, 24).contains("Apply filters"));
    for (i, value) in [
        "actual-ci-actor",
        "main",
        "push",
        "failure",
        "7",
        &"a".repeat(40),
        "2026-10-15",
    ]
    .iter()
    .enumerate()
    {
        ui(&mut app, read_ui_v2::ReadUiActionV2::Focus(i));
        paste(&mut app, value);
    }
    ui(&mut app, read_ui_v2::ReadUiActionV2::Apply);
    let work = take_work(&mut app).unwrap();
    assert_eq!(work.query_v2.as_ref().unwrap().page(), 1);
    let prepared = execute(work, &p, &f::database(&fix)).unwrap();
    assert!(!prepared.workspace.coverage.complete);
    assert_eq!(prepared.workspace.runs.len(), 1);
    unchanged(&fix, &sql, &git);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_invalid_workflow_filter_keeps_editor_and_sends_nothing() {
    let (fix, _, mut app, all) = f::setup(vec![]);
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenFilters);
    render(&mut app, 100, 24);
    ui(&mut app, read_ui_v2::ReadUiActionV2::Focus(4));
    paste(&mut app, "not-a-number");
    ui(&mut app, read_ui_v2::ReadUiActionV2::Apply);
    assert!(app.scoped_actions_work_v1.is_none());
    assert_eq!(app.overlay, Some(Overlay::ScopedActionsReadV2));
    assert!(app.status.contains("positive numeric"));
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_oversized_paste_is_rejected_before_editor_copy() {
    let (fix, _, mut app, all) = f::setup(vec![]);
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenFilters);
    render(&mut app, 100, 24);
    paste(&mut app, &"X".repeat(2 * 1024 * 1024));
    ui(&mut app, read_ui_v2::ReadUiActionV2::Apply);
    assert!(take_work(&mut app)
        .unwrap()
        .query_v2
        .as_ref()
        .unwrap()
        .filter()
        .actor
        .is_none());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_total_filter_input_is_bounded_without_hidden_truncation() {
    let (fix, _, mut app, all) = f::setup(vec![]);
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenFilters);
    render(&mut app, 100, 24);
    for i in 0..5 {
        ui(&mut app, read_ui_v2::ReadUiActionV2::Focus(i));
        paste(&mut app, &"x".repeat(1024));
    }
    // The fifth paste must be rejected whole, not silently partly accepted.
    ui(&mut app, read_ui_v2::ReadUiActionV2::Apply);
    assert!(app.scoped_actions_work_v1.is_none());
    assert!(app.status.contains("Invalid bounded filter"));
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_tiny_editor_has_close_but_no_usable_apply() {
    let (fix, _, mut app, all) = f::setup(vec![]);
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenFilters);
    let shown = render(&mut app, 40, 10);
    assert!(shown.contains("Resize for original Actions"));
    assert!(shown.contains("Close editor"));
    assert!(!shown.contains("Apply filters"));
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.scoped_actions_work_v1.is_none());
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_resize_retires_old_editor_hit_geometry_before_input() {
    let (fix, _, mut app, all) = f::setup(vec![]);
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenFilters);
    render(&mut app, 100, 24);
    app.handle_event(Event::Resize(40, 10));
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 12,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(app.scoped_actions_work_v1.is_none());
    ui(&mut app, read_ui_v2::ReadUiActionV2::Apply);
    assert!(app.scoped_actions_work_v1.is_none());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_keyboard_and_actual_hit_apply_same_original_filter() {
    for mouse in [false, true] {
        let (fix, _, mut app, all) = f::setup(vec![]);
        ui(&mut app, read_ui_v2::ReadUiActionV2::OpenFilters);
        render(&mut app, 100, 24);
        paste(&mut app, "actual-ci-actor");
        if mouse {
            app.handle_event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 3,
                row: 12,
                modifiers: KeyModifiers::NONE,
            }));
        } else {
            key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        }
        let work = take_work(&mut app).expect("actual keyboard/click Apply");
        assert_eq!(
            work.query_v2.as_ref().unwrap().filter().actor.as_deref(),
            Some("actual-ci-actor")
        );
        f::exact(&fix, &all);
    }
}
#[test]
fn scoped_actions_read_v2_editor_rejects_replaced_source_before_apply() {
    let (fix, _, mut app, all) = f::setup(vec![]);
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenFilters);
    render(&mut app, 100, 24);
    paste(&mut app, "alice");
    app.remote_scope = Some(Arc::new(
        app.remote_scope.as_ref().unwrap().as_ref().clone(),
    ));
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.scoped_actions_work_v1.is_none());
    assert!(app.overlay.is_none());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_search_genuine_snapshot_exact_positions_no_gh_sql_git() {
    let (fix, p, mut app, all) = loaded(vec![]);
    let sql = f::sql_state(&fix);
    let git = f::git_state(&fix);
    let prepared = search(&mut app, &p, &fix, "error", M::ExactText).unwrap();
    assert_eq!(prepared.matches_v2.as_ref().unwrap().indices(), &[1, 3]);
    assert_eq!(prepared.workspace.selected, 3);
    assert_eq!(prepared.workspace.top, 1);
    assert_eq!(prepared.transport_bytes_v2, 0);
    assert_eq!(
        prepared
            .matches_v2
            .as_ref()
            .unwrap()
            .snapshot()
            .run()
            .run()
            .attempt,
        2
    );
    assert_eq!(
        prepared.matches_v2.as_ref().unwrap().snapshot().job().id(),
        501
    );
    unchanged(&fix, &sql, &git);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_regex_uses_shared_unicode_ordering_and_one_row_per_line() {
    let (fix, p, mut app, all) = loaded(vec![]);
    let prepared = search(&mut app, &p, &fix, "界|error|é", M::Regex).unwrap();
    assert_eq!(prepared.matches_v2.as_ref().unwrap().indices(), &[1, 3]);
    assert!(prepared.workspace.rows[1].label.contains("界 é"));
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_invalid_regex_keeps_actual_old_rows_matches_and_source() {
    let (fix, p, mut app, all) = loaded(vec![]);
    let value = search(&mut app, &p, &fix, "error", M::ExactText).unwrap();
    f::install(&mut app, value);
    let original = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .matches_v2
        .clone()
        .unwrap();
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    let state = app.scoped_actions_v1.as_mut().unwrap();
    state.epoch += 1;
    let work = ScopedActionsWorkV1::capture_search_v2(
        state.page.clone(),
        "[".into(),
        M::Regex,
        (0, 0),
        state.epoch,
    )
    .unwrap();
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    assert!(execute(work, &p, &f::database(&fix)).is_err());
    assert!(fail(&mut app, &owner));
    assert!(Arc::ptr_eq(
        app.scoped_actions_v1
            .as_ref()
            .unwrap()
            .matches_v2
            .as_ref()
            .unwrap(),
        &original
    ));
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
    assert!(app.status.contains("previous matches unchanged"));
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_empty_query_preserves_existing_shared_match_semantics() {
    let (fix, p, mut app, all) = loaded(vec![]);
    let prepared = search(&mut app, &p, &fix, "", M::ExactText).unwrap();
    assert_eq!(
        prepared.matches_v2.as_ref().unwrap().indices(),
        &[0, 1, 2, 3, 4]
    );
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_query_limit_and_controls_refuse_no_renewal_or_external_child() {
    let (fix, p, mut app, all) = loaded(vec![]);
    for query in ["x".repeat(1025), "line\nbreak".into(), "nul\0query".into()] {
        assert!(search(&mut app, &p, &fix, &query, M::ExactText).is_err());
    }
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_canceled_search_refuses_before_matching_or_commands() {
    let (fix, p, app, all) = loaded(vec![]);
    let state = app.scoped_actions_v1.as_ref().unwrap();
    let work = ScopedActionsWorkV1::capture_search_v2(
        state.page.clone(),
        "error".into(),
        M::ExactText,
        (0, 0),
        state.epoch,
    )
    .unwrap();
    work.cancellation().cancel();
    assert!(execute(work, &p, &f::database(&fix)).is_err());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_expired_search_has_no_fresh_clock_or_external_child() {
    let (fix, p, mut app, all) = loaded(vec![]);
    let state = app.scoped_actions_v1.as_mut().unwrap();
    let mut work = ScopedActionsWorkV1::capture_search_v2(
        state.page.clone(),
        "error".into(),
        M::ExactText,
        (0, 0),
        state.epoch,
    )
    .unwrap();
    Arc::get_mut(&mut work).unwrap().deadline = Instant::now() - Duration::from_millis(1);
    assert!(execute(work, &p, &f::database(&fix)).is_err());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_next_previous_clear_are_local_and_preserve_row_allocations() {
    let (fix, p, mut app, all) = loaded(vec![]);
    let value = search(&mut app, &p, &fix, "error", M::ExactText).unwrap();
    f::install(&mut app, value);
    let pointer = app.actions.as_ref().unwrap().rows.as_ptr();
    ui(&mut app, read_ui_v2::ReadUiActionV2::Next(true));
    assert_eq!(app.actions.as_ref().unwrap().selected, 1);
    ui(&mut app, read_ui_v2::ReadUiActionV2::Next(false));
    assert_eq!(app.actions.as_ref().unwrap().selected, 3);
    ui(&mut app, read_ui_v2::ReadUiActionV2::Clear);
    assert!(app.scoped_actions_v1.as_ref().unwrap().matches_v2.is_none());
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), pointer);
    assert_eq!(
        app.actions.as_ref().unwrap().rows[1].role,
        crate::theme::SemanticToken::Normal
    );
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_search_results_do_not_grow_moved_back_stack() {
    let (fix, p, mut app, all) = loaded(vec![]);
    let count = app.scoped_actions_v1.as_ref().unwrap().back.len();
    for query in ["error", "actual", "last"] {
        let value = search(&mut app, &p, &fix, query, M::ExactText).unwrap();
        f::install(&mut app, value);
        assert_eq!(app.scoped_actions_v1.as_ref().unwrap().back.len(), count);
    }
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_back_moves_matching_state_with_exact_original_snapshot() {
    let (fix, p, mut app, all) = loaded(vec![]);
    let value = search(&mut app, &p, &fix, "error", M::ExactText).unwrap();
    f::install(&mut app, value);
    let matches = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .matches_v2
        .clone()
        .unwrap();
    let pointer = app.actions.as_ref().unwrap().rows.as_ptr();
    let selected = app.actions.as_ref().unwrap().selected;
    let value = f::read(&mut app, &p, &fix, ActionsAction::View(ActionsView::Jobs)).unwrap();
    f::install(&mut app, value);
    assert!(intercept(&mut app, &ActionsAction::Back));
    assert!(Arc::ptr_eq(
        app.scoped_actions_v1
            .as_ref()
            .unwrap()
            .matches_v2
            .as_ref()
            .unwrap(),
        &matches
    ));
    assert_eq!(app.actions.as_ref().unwrap().selected, selected);
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), pointer);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_workspace_log_string_cannot_construct_a_snapshot_or_query() {
    let (fix, _, mut app, all) = f::opened(vec![]);
    app.actions.as_mut().unwrap().view = ActionsView::Logs;
    app.actions.as_mut().unwrap().run.as_mut().unwrap().logs = Some(TEXT.into());
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenSearch);
    assert_ne!(app.overlay, Some(Overlay::ScopedActionsReadV2));
    assert!(app.scoped_actions_work_v1.is_none());
    assert!(app.status.contains("workspace String") || app.notice.contains("workspace String"));
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_watch_is_initially_not_due_and_stop_cancels_original_token() {
    let (fix, _, mut app, all) = f::opened(vec![]);
    ui(&mut app, read_ui_v2::ReadUiActionV2::Watch);
    let watch = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .watch_v2
        .clone()
        .unwrap();
    let deadline = watch.deadline();
    assert!(!watch.due());
    read_ui_v2::tick(&mut app);
    assert!(app.scoped_actions_work_v1.is_none());
    ui(&mut app, read_ui_v2::ReadUiActionV2::Watch);
    assert!(!watch.active());
    assert!(watch.cancellation().is_cancelled());
    assert_eq!(deadline, watch.deadline());
    assert!(!app.actions.as_ref().unwrap().watching);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_watch_twelve_real_groups_keep_pin_token_and_deadline() {
    let mut tail = vec![];
    for _ in 0..12 {
        tail.extend(logs(42));
    }
    let (fix, p, mut app, all) = loaded(tail);
    ui(&mut app, read_ui_v2::ReadUiActionV2::Watch);
    let watch = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .watch_v2
        .clone()
        .unwrap();
    let deadline = watch.deadline();
    let old_back = app.scoped_actions_v1.as_ref().unwrap().back.len();
    for count in 1..=12 {
        let state = app.scoped_actions_v1.as_mut().unwrap();
        state.epoch += 1;
        let work =
            ScopedActionsWorkV1::capture_watch_v2(state.page.clone(), watch.clone(), state.epoch)
                .unwrap();
        assert!(work.cancellation().same_instance(watch.cancellation()));
        assert!(work.deadline() <= deadline);
        let prepared = execute(work, &p, &f::database(&fix)).unwrap();
        assert_eq!(prepared.watch_progress_v2.unwrap().0, count);
        assert!(watch.matches(&prepared.page));
        f::install(&mut app, prepared);
        assert_eq!(app.scoped_actions_v1.as_ref().unwrap().back.len(), old_back);
    }
    assert!(!watch.active());
    assert!(watch.cancellation().is_cancelled());
    assert!(watch
        .claim(&app.scoped_actions_v1.as_ref().unwrap().page)
        .is_err());
    assert_eq!(watch.deadline(), deadline);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_watch_source_replacement_stops_before_next_group() {
    let (fix, _, mut app, all) = f::opened(vec![]);
    ui(&mut app, read_ui_v2::ReadUiActionV2::Watch);
    let watch = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .watch_v2
        .clone()
        .unwrap();
    let (other, _, other_app, other_steps) = f::setup(vec![]);
    app.remote_scope_source = other_app.remote_scope_source.clone();
    read_ui_v2::tick(&mut app);
    assert!(app.scoped_actions_v1.is_none());
    assert!(watch.cancellation().is_cancelled());
    assert!(app.scoped_actions_work_v1.is_none());
    f::exact(&fix, &all);
    f::exact(&other, &other_steps);
}
#[test]
fn scoped_actions_read_v2_watch_editor_open_stops_same_token_without_new_source() {
    let (fix, _, mut app, all) = loaded(vec![]);
    let source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
    ui(&mut app, read_ui_v2::ReadUiActionV2::Watch);
    let watch = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .watch_v2
        .clone()
        .unwrap();
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenSearch);
    assert!(watch.cancellation().is_cancelled());
    assert!(Arc::ptr_eq(
        &source,
        &app.scoped_actions_v1.as_ref().unwrap().source
    ));
    assert_eq!(app.overlay, Some(Overlay::ScopedActionsReadV2));
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_watch_failed_group_stops_shared_quota_without_retry() {
    let mut held = f::operation(ScopedActionsRequestKindV1::Attempt, 43);
    let (fix, p, mut app, all) = f::opened(std::mem::take(&mut held));
    ui(&mut app, read_ui_v2::ReadUiActionV2::Watch);
    let state = app.scoped_actions_v1.as_ref().unwrap();
    let watch = state.watch_v2.clone().unwrap();
    let work =
        ScopedActionsWorkV1::capture_watch_v2(state.page.clone(), watch.clone(), state.epoch)
            .unwrap();
    assert!(execute(work, &p, &f::database(&fix)).is_err());
    assert!(!watch.active());
    read_ui_v2::tick(&mut app);
    assert!(app.scoped_actions_work_v1.is_none());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_watch_cannot_adopt_another_actual_run_page() {
    let (fix, _, mut app, all) = f::opened(vec![]);
    ui(&mut app, read_ui_v2::ReadUiActionV2::Watch);
    let watch = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .watch_v2
        .clone()
        .unwrap();
    let original = app.scoped_actions_v1.as_ref().unwrap().back[0].page.clone();
    assert!(!watch.matches(&original));
    assert!(ScopedActionsWorkV1::capture_watch_v2(original, watch, 1).is_err());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_read_keys_do_not_steal_existing_runners_or_annotations() {
    let (fix, _, mut app, all) = loaded(vec![]);
    let event = Event::Key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE));
    assert!(!read_ui_v2::event(&mut app, &event));
    let mapped = crate::actions_ui::keyboard_action(
        KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE),
        app.actions.as_ref().unwrap(),
    );
    assert_eq!(mapped, Some(ActionsAction::Annotations(501)));
    let event = Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert!(!read_ui_v2::event(&mut app, &event));
    assert_eq!(
        crate::actions_ui::keyboard_action(
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
            app.actions.as_ref().unwrap()
        ),
        Some(ActionsAction::View(ActionsView::Runners))
    );
    f::exact(&fix, &all);
}

#[derive(Debug, PartialEq, Eq)]
struct EditorEffectsV3 {
    status: String,
    notice: String,
    overlay: Option<Overlay>,
    hits: Vec<(ratatui::layout::Rect, Action)>,
    focus: usize,
    loading: [bool; 5],
    cancel: bool,
    epoch: u64,
    source: usize,
    page: usize,
    rows: usize,
    work: Option<usize>,
    owner: Option<usize>,
    editor: Option<usize>,
    quit: bool,
}
fn editor_effects_v3(app: &App) -> EditorEffectsV3 {
    let state = app
        .scoped_actions_v1
        .as_ref()
        .expect("genuine current state");
    EditorEffectsV3 {
        status: app.status.clone(),
        notice: app.notice.clone(),
        overlay: app.overlay,
        hits: app
            .hits
            .iter()
            .map(|hit| (hit.rect, hit.action.clone()))
            .collect(),
        focus: app.focus,
        loading: app.loading,
        cancel: app.scoped_actions_cancel_v1,
        epoch: state.epoch,
        source: Arc::as_ptr(&state.source) as usize,
        page: Arc::as_ptr(&state.page) as usize,
        rows: app.actions.as_ref().unwrap().rows.as_ptr() as usize,
        work: app
            .scoped_actions_work_v1
            .as_ref()
            .map(|work| Arc::as_ptr(work) as usize),
        owner: app
            .scoped_actions_owner_v1
            .as_ref()
            .map(|owner| Arc::as_ptr(owner) as usize),
        editor: state
            .editor_v2
            .as_ref()
            .map(|editor| editor as *const _ as usize),
        quit: app.quit,
    }
}
fn whole_frame_v3(app: &mut App) {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
}

#[test]
fn scoped_actions_read_v2_editor_stale_close_apply_quit_preserve_actual_new_notice_hits() {
    let (fix, _, mut app, all) = loaded(vec![]);
    let sql = f::sql_state(&fix);
    let git = f::git_state(&fix);
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenSearch);
    render(&mut app, 100, 24);
    paste(&mut app, "old query");
    app.dispatch(Action::Explain("INDEPENDENT_NEW_NOTICE".into()));
    whole_frame_v3(&mut app);
    assert!(app.hits.iter().any(|hit| hit.action == Action::Close));
    let before = editor_effects_v3(&app);
    for stale in [
        read_ui_v2::ReadUiActionV2::Close,
        read_ui_v2::ReadUiActionV2::Apply,
        read_ui_v2::ReadUiActionV2::Quit,
    ] {
        ui(&mut app, stale);
        assert_eq!(editor_effects_v3(&app), before);
    }
    // Ordinary real new-overlay input remains usable; no stale editor action
    // is claimed to be generated by this key/mouse route.
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    assert!(!app.quit);
    unchanged(&fix, &sql, &git);
    f::exact(&fix, &all);
}

#[test]
fn scoped_actions_read_v2_editor_open_newer_foreign_intent_preserves_original_watch_effects() {
    for thread in [false, true] {
        let (fix, _, mut app, all) = loaded(vec![]);
        let sql = f::sql_state(&fix);
        let git = f::git_state(&fix);
        ui(&mut app, read_ui_v2::ReadUiActionV2::Watch);
        let watch = app
            .scoped_actions_v1
            .as_ref()
            .unwrap()
            .watch_v2
            .clone()
            .unwrap();
        whole_frame_v3(&mut app);
        if thread {
            app.scoped_threads_cancel_v1 = true;
        } else {
            app.scoped_reviews_cancel_v2 = true;
        }
        app.status = "INDEPENDENT_NEWER_FOREIGN_INTENT".into();
        let before = editor_effects_v3(&app);
        ui(&mut app, read_ui_v2::ReadUiActionV2::OpenSearch);
        assert_eq!(editor_effects_v3(&app), before);
        assert!(watch.active());
        assert!(!watch.cancellation().is_cancelled());
        assert!(app.actions.as_ref().unwrap().watching);
        unchanged(&fix, &sql, &git);
        f::exact(&fix, &all);
    }
}

#[test]
fn scoped_actions_read_v2_editor_unavailable_kind_does_not_stop_watch_before_normal_overlay_tick() {
    for (actual_logs, shortcut) in [(true, 'f'), (false, 'f'), (false, '/')] {
        let (fix, _, mut app, all) = if actual_logs {
            loaded(vec![])
        } else {
            f::opened(vec![])
        };
        let sql = f::sql_state(&fix);
        let git = f::git_state(&fix);
        ui(&mut app, read_ui_v2::ReadUiActionV2::Watch);
        let state = app.scoped_actions_v1.as_ref().unwrap();
        let watch = state.watch_v2.clone().unwrap();
        let epoch = state.epoch;
        let page = state.page.clone();
        let cancel = app.scoped_actions_cancel_v1;
        key(&mut app, KeyCode::Char(shortcut), KeyModifiers::NONE);
        assert_eq!(app.overlay, Some(Overlay::Notice));
        assert!(app.scoped_actions_v1.as_ref().unwrap().editor_v2.is_none());
        assert!(watch.active());
        assert!(!watch.cancellation().is_cancelled());
        assert_eq!(app.scoped_actions_v1.as_ref().unwrap().epoch, epoch);
        assert_eq!(app.scoped_actions_cancel_v1, cancel);
        assert!(app.actions.as_ref().unwrap().watching);
        assert!(Arc::ptr_eq(
            &app.scoped_actions_v1.as_ref().unwrap().page,
            &page
        ));
        // Existing explicit no-Watch-behind-overlays policy is unchanged.
        read_ui_v2::tick(&mut app);
        assert!(watch.cancellation().is_cancelled());
        assert!(!app.actions.as_ref().unwrap().watching);
        assert!(app.scoped_actions_work_v1.is_none());
        unchanged(&fix, &sql, &git);
        f::exact(&fix, &all);
    }
}

#[test]
fn scoped_actions_read_v2_editor_valid_search_key_actual_hit_stops_same_watch_then_captures_current_epoch(
) {
    for mouse in [false, true] {
        let (fix, p, mut app, all) = loaded(vec![]);
        let sql = f::sql_state(&fix);
        let git = f::git_state(&fix);
        ui(&mut app, read_ui_v2::ReadUiActionV2::Watch);
        let state = app.scoped_actions_v1.as_ref().unwrap();
        let watch = state.watch_v2.clone().unwrap();
        let page = state.page.clone();
        let epoch = state.epoch;
        if mouse {
            // Actual existing shared renderer/control paging, not fabricated
            // coordinates or a direct preselected handler result.
            let action = ActionsAction::ScopedReadV2(R::Ui(read_ui_v2::ReadUiActionV2::OpenSearch));
            let mut found = None;
            for controls_page in 0..64 {
                app.actions.as_mut().unwrap().controls_page = controls_page;
                whole_frame_v3(&mut app);
                found = app
                    .hits
                    .iter()
                    .find(|hit| hit.action == Action::Actions(action.clone()))
                    .map(|hit| hit.rect);
                if found.is_some() {
                    break;
                }
            }
            let rect = found.expect("actual shared Search loaded logs button");
            app.handle_event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: KeyModifiers::NONE,
            }));
        } else {
            key(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
        }
        assert!(watch.cancellation().is_cancelled());
        assert_eq!(app.overlay, Some(Overlay::ScopedActionsReadV2));
        assert_eq!(
            app.scoped_actions_v1.as_ref().unwrap().epoch,
            epoch.wrapping_add(1)
        );
        render(&mut app, 100, 24);
        paste(&mut app, "error");
        ui(&mut app, read_ui_v2::ReadUiActionV2::Apply);
        let work = take_work(&mut app).expect("genuine current editor Work");
        assert!(Arc::ptr_eq(&work.page, &page));
        assert_eq!(work.epoch, epoch.wrapping_add(2));
        let result = execute(work, &p, &f::database(&fix)).unwrap();
        assert_eq!(result.matches_v2.as_ref().unwrap().indices(), &[1, 3]);
        unchanged(&fix, &sql, &git);
        f::exact(&fix, &all);
    }
}

#[test]
fn scoped_actions_read_v2_editor_stale_apply_and_busy_open_preserve_genuine_new_owner() {
    let (fix, _, mut app, all) = loaded(vec![]);
    let sql = f::sql_state(&fix);
    let git = f::git_state(&fix);
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenSearch);
    render(&mut app, 100, 24);
    paste(&mut app, "OLD_EDITOR_QUERY");
    let state = app.scoped_actions_v1.as_mut().unwrap();
    state.epoch = state.epoch.wrapping_add(1);
    let work = ScopedActionsWorkV1::capture_search_v2(
        state.page.clone(),
        "error".into(),
        M::ExactText,
        (2, 1),
        state.epoch,
    )
    .unwrap();
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    let token = owner.cancellation().clone();
    app.scoped_actions_work_v1 = Some(work.clone());
    app.scoped_actions_owner_v1 = Some(owner.clone());
    app.loading[View::Actions.index()] = true;
    app.status = "GENUINE_INDEPENDENT_NEW_ACTIONS_OWNER".into();
    let epoch = app.scoped_actions_v1.as_ref().unwrap().epoch;
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    // Actual stale modal key retires ONLY its own old overlay/geometry.
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    assert!(app.scoped_actions_v1.as_ref().unwrap().editor_v2.is_none());
    assert_eq!(app.scoped_actions_v1.as_ref().unwrap().epoch, epoch);
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
    assert!(Arc::ptr_eq(
        app.scoped_actions_work_v1.as_ref().unwrap(),
        &work
    ));
    assert!(Arc::ptr_eq(
        app.scoped_actions_owner_v1.as_ref().unwrap(),
        &owner
    ));
    assert!(!token.is_cancelled());
    assert!(app.loading[View::Actions.index()]);
    assert_eq!(app.status, "GENUINE_INDEPENDENT_NEW_ACTIONS_OWNER");
    let before = editor_effects_v3(&app);
    ui(&mut app, read_ui_v2::ReadUiActionV2::OpenSearch);
    assert_eq!(editor_effects_v3(&app), before);
    assert!(!token.is_cancelled());
    unchanged(&fix, &sql, &git);
    f::exact(&fix, &all);
}
