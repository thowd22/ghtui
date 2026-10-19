//! Executed integration corrections use actual strict factories, not DTO proof.
use super::*;

#[test]
fn corrective_initial_closed_matched_controls_mouse_watch_and_keyboard_stop_are_same_semantics() {
    let (f, steps) = script(vec![]);
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let mut found = None;
    for _ in 0..32 {
        frame(&mut app);
        if let Some(hit) = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::Actions(ActionsAction::Watch))
        {
            found = Some(hit.rect);
            break;
        }
        app.dispatch(Action::Actions(ActionsAction::ControlsPage(1)));
    }
    let hit = found.expect(
        "Initial closed matched factory must expose the admitted clickable Watch, not Unavailable",
    );
    app.handle_event(Event::Mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: hit.x,
        row: hit.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(app.pr_check_watch_v2.as_ref().unwrap().active());
    assert!(!app.actions.as_ref().unwrap().watching);
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('w'),
        KeyModifiers::NONE,
    )));
    assert!(app.pr_check_watch_v2.is_none());
    assert!(app.form.is_none());
    read_only(&f, &before, &git, &steps);
}

#[test]
fn corrective_pending_escape_and_actual_footer_hit_use_same_cancel_not_back() {
    for mouse in [false, true] {
        let (f, steps) = capture_script(502);
        let p = provider(&f);
        let mut app = captured(&f, &p, 502);
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let original_page = app.github.as_ref().unwrap().pr.as_ref().unwrap() as *const _;
        let route = app.pr_checks_route.clone().unwrap();
        next::queue_search(
            &mut app,
            route,
            "line2".into(),
            LoadedLogSearchModeV2::ExactText,
        );
        app.loading[View::Actions.index()] = true;
        assert!(frame(&mut app).contains("  Esc Cancel  ? Help"));
        let event = if mouse {
            let hit = app
                .hits
                .iter()
                .find(|hit| hit.action == Action::Cancel)
                .unwrap();
            Event::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: hit.rect.x,
                row: hit.rect.y,
                modifiers: KeyModifiers::NONE,
            })
        } else {
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        };
        app.handle_event(event);
        assert!(app.cancel_requested);
        assert_eq!(
            app.view,
            View::Actions,
            "Cancel is not cached Back navigation"
        );
        assert!(app.pr_checks_route.is_none());
        assert!(app.actions.is_none());
        assert_eq!(
            app.github.as_ref().unwrap().pr.as_ref().unwrap() as *const _,
            original_page
        );
        read_only(&f, &before, &git, &steps);
    }
}

#[test]
fn corrective_detached_origin_global_refresh_hides_loaded_private_same_dispatch_and_draw() {
    let (f, steps) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    assert!(frame(&mut app).contains("OTHER_JOB_PRIVATE_LOG_CANARY"));
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    app.remote_scope_source = None;
    app.dispatch(Action::Refresh);
    hidden_now(&mut app);
    read_only(&f, &before, &git, &steps);
}

#[test]
fn corrective_final_original_cancel_after_closed_proof_and_pure_preparation_has_no_private_completion(
) {
    let (f, steps) = script(annotations(501, vec![annotation_row()], Some(1)));
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    assert!(next::intercept(&mut app, &ActionsAction::Annotations(501)));
    let work = corrective::take_work(&mut app).unwrap();
    let owner = work.owner().clone();
    let cancel = CancellationToken::default();
    let value = next::execute_after_preparation_for_test(
        work,
        &p,
        &database(&f),
        &Default::default(),
        &cancel,
        || {
            exact(&f, &steps); // ALL final children completed; no extra proof.
            assert_eq!(sql_state(&f), before);
            cancel.cancel(); // Genuine original token, after bounded preparation.
        },
    );
    assert!(corrective::publish(&mut app, &owner, value));
    hidden_now(&mut app);
    read_only(&f, &before, &git, &steps);
}

#[test]
fn corrective_clear_matches_retires_only_matching_queued_and_completed_pure_search() {
    for completed in [false, true] {
        let (f, steps) = capture_script(502);
        let p = provider(&f);
        let mut app = captured(&f, &p, 502);
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let route = app.pr_checks_route.clone().unwrap();
        next::queue_search(
            &mut app,
            route.clone(),
            "line2".into(),
            LoadedLogSearchModeV2::ExactText,
        );
        let owner = app.pr_check_continuation_owner_v2.clone().unwrap();
        let late = completed.then(|| {
            let work = corrective::take_work(&mut app).unwrap();
            next::execute(
                work,
                &p,
                &database(&f),
                &Default::default(),
                &Default::default(),
            )
            .unwrap()
        });
        ui::dispatch(&mut app, ui::PrCheckContinueUiV2::ClearMatches);
        assert!(app.pr_check_continuation_owner_v2.is_none());
        assert!(app.pr_check_continuation_request_v2.is_none());
        assert!(app.pr_check_continuation_cancel_v2);
        if let Some(late) = late {
            assert!(!corrective::publish(&mut app, &owner, Ok(late)));
        }
        assert!(app.pr_check_log_matches_v2.is_none());
        assert!(Arc::ptr_eq(&route, app.pr_checks_route.as_ref().unwrap()));
        assert!(next::intercept(&mut app, &ActionsAction::Watch));
        let watch = app.pr_check_watch_v2.clone().unwrap();
        ui::dispatch(&mut app, ui::PrCheckContinueUiV2::ClearMatches);
        assert!(Arc::ptr_eq(&watch, app.pr_check_watch_v2.as_ref().unwrap()));
        assert!(watch.active());
        read_only(&f, &before, &git, &steps);
    }
}
