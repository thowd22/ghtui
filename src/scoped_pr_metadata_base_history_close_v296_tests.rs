//! Genuine confirmed publication/history and a separate normal Base Open.
use super::*;
use crate::scoped_pr_collaboration_ui_v17::history_close_v296 as close;

fn confirmed_history(later: Vec<GhStep>) -> Setup {
    let mut calls = paid();
    calls.extend(history_read());
    calls.extend(later);
    let (mut s, form, preview) = prepared(calls);
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::Confirmed);
    assert!(!outcome.draft_retained());
    assert_marker(&s, &form, &outcome, PrWriteState::Confirmed);
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let listed = base::history::open(
        original.clone(),
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
        None,
    )
    .unwrap();
    assert_eq!(
        base::history::state(&listed).unwrap().view.snapshot.rows[0]
            .marker
            .state,
        PrWriteState::Confirmed
    );
    assert!(
        base::history::request(&listed).is_err(),
        "Confirmed history is not an unresolved Inspect request"
    );
    s.app.form = Some(listed);
    s.app.overlay = Some(crate::app::Overlay::Form);
    s.app.form_dismissed = false;
    s
}
fn close_key(s: &mut Setup) {
    s.app.handle_event(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
}
fn traffic(s: &Setup) -> Vec<u8> {
    fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap()
}
#[test]
fn pr_metadata_base_v32_history_close_confirmed_actual_form_then_fresh_normal_open_is_unapproved() {
    let mut s = confirmed_history(catalogue_phase(
        vec![json!([
            branch("main", &"b".repeat(40)),
            branch("next", TIP)
        ])],
        json!(true),
    ));
    let before = whole_sql_v279(&s);
    let before_traffic = traffic(&s);
    assert!(
        close::capture(&s.app).is_some(),
        "Confirmed List uses its actual view/ticket, not unresolved request()"
    );
    assert!(!ui::pure_close_v20::can_close(&s.app));
    close_key(&mut s);
    assert!(s.app.form.is_none());
    assert!(s.app.overlay.is_none());
    assert!(s.app.form_dismissed);
    assert!(s.app.closed_conversation_draft_v20.is_none());
    assert_eq!(s.app.status, "Composer closed • draft kept locally");
    assert_eq!(whole_sql_v279(&s), before);
    assert_eq!(traffic(&s), before_traffic, "No Close network call");
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let mut fresh = base::open(
        original.clone(),
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    assert!(fresh.publication.is_none());
    assert!(!fresh.preview);
    assert!(!fresh.submitting);
    assert!(fresh
        .fields
        .iter()
        .all(|field| field.editor.text.is_empty()));
    assert!(base::state(&fresh).unwrap().selection.is_none());
    assert!(base::request(&fresh).is_err());
    let after_open = traffic(&s);
    fresh.dispatch(&FormAction::MetadataPicker(
        crate::pr_metadata_picker::PrMetadataPickerAction::Close,
    ));
    fresh.dispatch(&FormAction::Preview);
    assert!(
        base::request(&fresh).is_err(),
        "Close never restores the historical selected branch or approval"
    );
    assert_eq!(traffic(&s), after_open);
    s.app.form = Some(fresh);
    s.app.overlay = Some(crate::app::Overlay::Form);
    s.app.form_dismissed = false;
    assert!(
        close::capture(&s.app).is_none(),
        "A genuine editable Base is never a history Close proof"
    );
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_history_close_rechecks_same_form_and_current_account_after_generic_close() {
    for case in 0..5 {
        let mut s = confirmed_history(Vec::new());
        let proof = close::capture(&s.app).unwrap();
        let presentation = Arc::clone(s.app.form.as_ref().unwrap().presentation_v4().0);
        let before = whole_sql_v279(&s);
        let before_traffic = traffic(&s);
        // Stop at the ordinary Close boundary, then adversarially alter one
        // exact post-Close predicate before consuming the captured proof.
        let form = s.app.form.as_mut().unwrap();
        form.dispatch(&FormAction::Close);
        form.invalidate_plan();
        s.app.overlay = None;
        s.app.form_dismissed = true;
        match case {
            0 => {
                s.app.form.as_mut().unwrap().fields.push(
                    crate::forms::Field::text("graft", "Not history", "private", false).unwrap(),
                )
            }
            1 => s.app.github.as_mut().unwrap().identity.account_id = Some(43),
            2 => s.app.scoped_threads_cancel_v1 = true,
            3 => s.app.form.as_mut().unwrap().editor_requested = true,
            4 => s.app.form.as_mut().unwrap().preview_requested = true,
            _ => unreachable!(),
        }
        assert!(!close::form_closed(&mut s.app, proof), "case {case}");
        assert!(Arc::ptr_eq(
            s.app.form.as_ref().unwrap().presentation_v4().0,
            &presentation
        ));
        assert_eq!(whole_sql_v279(&s), before);
        assert_eq!(traffic(&s), before_traffic);
        exact_mixed(&s, s.steps.len());
    }
}
