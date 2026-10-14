//! Genuine closed original Source/PR/Checks factories; navigation adds no read.
use super::*;

fn raw_steps(known: bool) -> Value {
    let mut steps = vec![
        json!({"number":1,"name":"Compile exact source","status":"completed","conclusion":"success"}),
        json!({"number":0,"name":"Unknown reported number","status":"completed","conclusion":"failure"}),
        json!({"number":18,"name":"Unknown conclusion","status":"completed","conclusion":null}),
        json!({"number":19,"name":"Unknown completion","status":null,"conclusion":"failure"}),
        json!({"number":20,"name":"Still running","status":"in_progress","conclusion":"failure"}),
    ];
    if known {
        steps.insert(1, json!({"number":2,"name":"Test exact source","status":"completed","conclusion":"failure"}));
        steps.push(json!({"number":17,"name":"Sparse reported failure 界🦀","status":"completed","conclusion":"failure"}));
    }
    json!(steps)
}
fn fixture(known: bool, logs: bool) -> (Fixture, Vec<GhStep>) {
    let mut op = initial_choice(false, false);
    let mut member = job_id(123, 501);
    member["steps"] = raw_steps(known);
    op[9] = get(
        &format!("repos/{REPO}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
        json!({"total_count":2,"jobs":[member,job_id(123,502)]}),
    );
    if logs {
        op.extend(log_steps("WHOLE_PINNED_JOB_NOT_SELECTIVE_STEP\nline2\n"));
    }
    scripted(op)
}
fn jobs_app(f: &Fixture, p: &GithubProvider) -> App {
    let mut app = loaded(f, p, &Default::default());
    let prepared = apply_v1(
        app.pr_checks_route.clone().unwrap(),
        app.actions.take().unwrap(),
        ActionsAction::OpenJob(501),
        p,
        &database(f),
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
    install(&mut app, prepared);
    assert_eq!(
        app.actions.as_ref().unwrap().view,
        crate::actions_workspace::ActionsView::Jobs
    );
    app
}
fn frame(app: &mut App, width: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, 40)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
fn navigate(app: &mut App, mouse: bool, next: bool) {
    frame(app, 120);
    app.handle_event(if mouse {
        let hit = app
            .hits
            .iter()
            .find(|hit| {
                hit.action == Action::PrCheckContinueV2(ui::PrCheckContinueUiV2::FailedStep(next))
            })
            .unwrap();
        Event::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: hit.rect.x,
            row: hit.rect.y,
            modifiers: KeyModifiers::NONE,
        })
    } else {
        Event::Key(KeyEvent::new(
            KeyCode::Char(if next { '>' } else { '<' }),
            KeyModifiers::NONE,
        ))
    });
}

#[test]
fn failed_step_v8_actual_original_sparse_numbers_keyboard_mouse_wrap_no_effects() {
    for mouse in [false, true] {
        let (f, expected) = fixture(true, false);
        let p = provider(&f);
        let mut app = jobs_app(&f, &p);
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let route = app.pr_checks_route.clone().unwrap();
        let rows = app.actions.as_ref().unwrap().rows.as_ptr();
        for (next, number) in [(true, 2), (true, 17), (true, 2), (false, 17), (false, 2)] {
            navigate(&mut app, mouse, next);
            let w = app.actions.as_ref().unwrap();
            assert!(w.rows[w.selected]
                .label
                .contains(&format!("Step {number} •")));
            assert_eq!(
                w.rows[w.selected].role,
                crate::theme::SemanticToken::Failure
            );
            assert_eq!(
                w.rows[w.selected].action,
                Some(ActionsAction::Logs(Some(501)))
            );
            assert!(app
                .status
                .contains(&format!("Observed failed step {number}")));
            assert!(app.status.contains("original attempt 2"));
            assert!(frame(&mut app, 120).contains(&format!("Step {number} •")));
            assert!(app.actions_action.is_none());
            assert!(app.pr_check_continuation_request_v2.is_none());
            assert!(Arc::ptr_eq(&route, app.pr_checks_route.as_ref().unwrap()));
            assert_eq!(rows, app.actions.as_ref().unwrap().rows.as_ptr());
        }
        read_only(&f, &before, &git, &expected);
    }
}

#[test]
fn failed_step_v8_nullable_unknown_noncompleted_and_zero_never_become_failure() {
    for mouse in [false, true] {
        let (f, expected) = fixture(false, false);
        let p = provider(&f);
        let mut app = jobs_app(&f, &p);
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let selected = app.actions.as_ref().unwrap().selected;
        navigate(&mut app, mouse, true);
        assert_eq!(app.actions.as_ref().unwrap().selected, selected);
        assert!(frame(&mut app, 120).contains("No positively observed completed/failure step"));
        assert!(app
            .notice
            .contains("Missing/unknown status or number is not failure"));
        assert!(app.actions_action.is_none());
        read_only(&f, &before, &git, &expected);
    }
}

#[test]
fn failed_step_v8_disjoint_clickable_previous_next_controls_survive_40_80_120() {
    let (f, expected) = fixture(true, false);
    let p = provider(&f);
    let mut app = jobs_app(&f, &p);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    app.content[View::Actions.index()] = vec!["GENERIC_UNDERLAY_NOT_A_CONTROL".into(); 40];
    for width in [40, 80, 120] {
        let text = frame(&mut app, width);
        assert!(text.contains("[> Next failed step]"));
        assert!(text.contains("[< Previous failed step]"));
        let hits = app
            .hits
            .iter()
            .filter(|h| {
                matches!(
                    h.action,
                    Action::PrCheckContinueV2(ui::PrCheckContinueUiV2::FailedStep(_))
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(hits.len(), 2);
        for hit in &hits {
            assert!(hit.rect.right() <= width);
            let rendered_row = text
                .chars()
                .skip(usize::from(hit.rect.y * width))
                .take(usize::from(width))
                .collect::<String>();
            assert!(
                !rendered_row.contains("GENERIC_UNDERLAY"),
                "Reserved control strip clears underlying generic content"
            );
            for other in &app.hits {
                if std::ptr::eq(*hit, other) {
                    continue;
                }
                assert!(
                    !hit.rect.intersects(other.rect),
                    "No stale/underlaid control or content hit"
                );
            }
        }
        app.handle_event(Event::Resize(width, 40));
        frame(&mut app, width);
        let hit = app
            .hits
            .iter()
            .find(|hit| {
                hit.action == Action::PrCheckContinueV2(ui::PrCheckContinueUiV2::FailedStep(false))
            })
            .unwrap()
            .rect;
        app.handle_event(Event::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: hit.x,
            row: hit.y,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(app.status.contains("Observed failed step"));
    }
    read_only(&f, &before, &git, &expected);
}

#[test]
fn failed_step_v8_changed_attempt_uid_source_or_pending_read_refuses_local_jump() {
    for changed in ["attempt", "uid", "source", "pending"] {
        let (f, expected) = fixture(true, false);
        let p = provider(&f);
        let mut app = jobs_app(&f, &p);
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let selected = app.actions.as_ref().unwrap().selected;
        match changed {
            "attempt" => {
                app.actions
                    .as_mut()
                    .unwrap()
                    .run
                    .as_mut()
                    .unwrap()
                    .attempt
                    .run
                    .attempt = 3
            }
            "uid" => app.actions.as_mut().unwrap().identity.account_id = Some(43),
            "source" => app.remote_scope_source = None,
            _ => app.loading[View::Actions.index()] = true,
        }
        ui::dispatch(&mut app, ui::PrCheckContinueUiV2::FailedStep(true));
        assert_eq!(app.actions.as_ref().unwrap().selected, selected);
        assert!(app.notice.contains("No positively observed"));
        assert!(app.actions_action.is_none());
        read_only(&f, &before, &git, &expected);
    }
}

#[test]
fn failed_step_v8_enter_keeps_existing_fresh_whole_original_job_log_transport() {
    let (f, expected) = fixture(true, true);
    let p = provider(&f);
    let mut app = jobs_app(&f, &p);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    navigate(&mut app, false, true);
    assert!(app.status.contains("not selective step logs"));
    let prepared = apply_v1(
        app.pr_checks_route.clone().unwrap(),
        app.actions.take().unwrap(),
        ActionsAction::Logs(Some(501)),
        &p,
        &database(&f),
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
    install(&mut app, prepared);
    let capture = app
        .pr_checks_route
        .as_ref()
        .unwrap()
        .loaded_capture_v2()
        .unwrap();
    assert_eq!(capture.captured_target_v2().job_id(), 501);
    assert_eq!(capture.captured_target_v2().attempt(), 2);
    assert!(frame(&mut app, 120).contains("WHOLE_PINNED_JOB_NOT_SELECTIVE_STEP"));
    read_only(&f, &before, &git, &expected);
}
