//! DECLARED / UNRUN. Real Files/draft/preview factories and actual shared UI;
//! paired real-binary resize/disabled-control journeys remain mandatory.
use super::*;
use crate::{
    app::Action,
    forms::FormAction,
    scoped_pr_observed_tabs_v1::resources::{
        immutable_images::inline_v1::native_v1 as inline_native, threads_v1::native_v1 as native,
    },
};
use crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::{backend::TestBackend, style::Modifier, Terminal};

fn composer(preview: bool) -> Real {
    let mut steps = observation();
    if preview {
        steps.extend(observation());
    }
    let mut real = real(steps);
    let intent = file::FileCommentIntentV75::capture(&real.app).unwrap();
    let draft = file::FileCommentDraftV75::open(
        intent,
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        deadline(),
    )
    .unwrap();
    let mut form = file::form::form_from_draft(draft).unwrap();
    form.paste(FILE_BODY);
    if preview {
        let request = file::form::request(&form).unwrap();
        let prepared = file::FileCommentPreviewV75::prepare(
            request.intent().clone(),
            request.body().clone(),
            &provider(&real.fixture),
            &database(&real.fixture),
            &Default::default(),
            deadline(),
        )
        .unwrap();
        assert!(file::form::accept_preview(&mut form, &request, prepared));
    }
    real.app.form = Some(form);
    real.app.overlay = Some(Overlay::Form);
    real
}

fn paint(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal
}

fn text(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn file_comments_v82_foreign_view_source_overlay_or_pending_intent_cannot_consume_original_form_input(
) {
    for case in 0..19 {
        let mut real = composer(false);
        let original = file::form::request(real.app.form.as_ref().unwrap()).unwrap();
        let form = real.app.form.as_mut().unwrap();
        form.preview_requested = true;
        form.submit_requested = true;
        form.error = "Original error must remain untouched".into();
        match case {
            0 => real.app.view = View::Repository,
            1 => real.app.overlay = None,
            2 => real.app.overlay = Some(Overlay::Notice),
            3 => real.app.form_dismissed = true,
            4 => real.app.loading[View::Inbox.index()] = true,
            5 => real.app.selected += 1,
            6 => real.app.github.as_mut().unwrap().identity.account_id = Some(43),
            7 => real.app.remote_scope_source = None,
            8 => real.app.refresh_requested = true,
            9 => real.app.cancel_requested = true,
            10 => real.app.original_tree_cancel_v1 = true,
            11 => real.app.scoped_reviews_cancel_v2 = true,
            12 => real.app.scoped_threads_cancel_v1 = true,
            13 => real.app.scoped_inline_cancel_v1 = true,
            14 => real.app.pr_native_diff_cancel_v1 = true,
            15 => real.app.scoped_actions_cancel_v1 = true,
            16 => real.app.pr_check_continuation_cancel_v2 = true,
            17 => real.app.personal_pr_entry_cancel_v13 = true,
            18 => real.app.form.as_mut().unwrap().editor_requested = true,
            _ => unreachable!(),
        }
        real.app.status = "Foreign visible status must remain untouched".into();
        let hits: Vec<_> = real
            .app
            .hits
            .iter()
            .map(|hit| (hit.rect, hit.action.clone()))
            .collect();
        // No writer is supplied: the exact-original preflight must return
        // BEFORE even the missing-preview/writer error path consumes input.
        native::capture_form(&mut real.app);
        let form = real.app.form.as_ref().unwrap();
        assert!(
            form.preview_requested && form.submit_requested,
            "case {case}"
        );
        assert!(!form.submitting, "case {case}");
        assert_eq!(form.error, "Original error must remain untouched");
        assert!(original.matches_form(form), "case {case}");
        assert_eq!(
            real.app.status,
            "Foreign visible status must remain untouched"
        );
        assert_eq!(
            real.app
                .hits
                .iter()
                .map(|hit| (hit.rect, hit.action.clone()))
                .collect::<Vec<_>>(),
            hits
        );
        assert!(real.app.scoped_threads_work_v1.is_none());
        assert!(real.app.scoped_threads_owner_v1.is_none());
        intact(&real);
    }
}

#[test]
fn file_comments_v82_only_actual_installed_form_ignores_own_preview_bits_and_queues_same_original()
{
    let mut real = composer(false);
    real.app.form.as_mut().unwrap().preview_requested = true;
    let current = real.app.form.as_ref().unwrap();
    let cloned = current.clone();
    assert!(!inline_native::newer_intent_for_original_file_form_v82(
        &real.app, current
    ));
    assert!(inline_native::newer_intent_for_original_file_form_v82(
        &real.app, &cloned
    ));
    let request = file::form::request(current).unwrap();
    native::capture_form(&mut real.app);
    let queued = real.app.scoped_threads_work_v1.clone().unwrap();
    assert_eq!(queued.phase(), native::ScopedThreadPhaseV1::Preview);
    match &queued.intent {
        native::ThreadOperationOriginV1::Files(actual) => {
            assert!(Arc::ptr_eq(actual, &request.intent().original));
        }
        _ => panic!("FILE Preview must retain the same actual original Files intent"),
    }
    let form = real.app.form.as_ref().unwrap();
    assert!(form.submitting && !form.preview_requested && !form.submit_requested);
    assert!(request.matches_form(form));
    native::capture_form(&mut real.app);
    assert!(Arc::ptr_eq(
        &queued,
        real.app.scoped_threads_work_v1.as_ref().unwrap()
    ));
    intact(&real);
}

#[test]
fn file_comments_v82_preview_focus_is_actual_visible_footer_not_hidden_body_editor() {
    let mut real = composer(true);
    let form = real.app.form.as_ref().unwrap();
    assert_eq!(form.focus, form.fields.len());
    assert!(file::form::confirmed_request_v82(form).is_err());
    let terminal = paint(&mut real.app, 120, 40);
    let frame = text(&terminal);
    assert!(frame.contains("READ-ONLY PREVIEW") && frame.contains("[Confirm action]"));
    let form = real.app.form.as_ref().unwrap();
    assert!(form.field_rects.iter().all(|rect| rect.is_empty()));
    assert_eq!(form.focus, form.fields.len());
    let preview = real
        .app
        .hits
        .iter()
        .find(|hit| hit.action == Action::Form(FormAction::Preview))
        .unwrap();
    assert!(
        terminal.backend().buffer()[(preview.rect.x, preview.rect.y)]
            .modifier
            .contains(Modifier::REVERSED)
    );
    assert!(file::form::confirmed_request_v82(form).is_ok());
    let first = form.fields.len();
    let actions = [
        FormAction::Preview,
        FormAction::Editor,
        FormAction::Submit,
        FormAction::Close,
    ];
    for (code, expected) in [
        (KeyCode::Tab, first + 1),
        (KeyCode::Tab, first + 2),
        (KeyCode::Tab, first + 3),
        (KeyCode::Tab, first),
        (KeyCode::BackTab, first + 3),
        (KeyCode::BackTab, first + 2),
        (KeyCode::BackTab, first + 1),
        (KeyCode::BackTab, first),
    ] {
        let form = real.app.form.as_mut().unwrap();
        assert!(form.key(code, KeyModifiers::NONE).is_none());
        assert_eq!(form.focus, expected);
        let terminal = paint(&mut real.app, 120, 40);
        let hit = real
            .app
            .hits
            .iter()
            .find(|hit| hit.action == Action::Form(actions[expected - first].clone()))
            .unwrap();
        assert!(terminal.backend().buffer()[(hit.rect.x, hit.rect.y)]
            .modifier
            .contains(Modifier::REVERSED));
    }
    intact(&real);
}

#[test]
fn file_comments_v82_resize_after_queued_submit_refuses_then_real_repaint_requires_explicit_confirm(
) {
    let mut real = composer(true);
    paint(&mut real.app, 120, 40);
    let original = file::form::confirmed_request_v82(real.app.form.as_ref().unwrap()).unwrap();
    real.app.dispatch(Action::Form(FormAction::Submit));
    assert!(real.app.form.as_ref().unwrap().submit_requested);
    real.app.handle_event(Event::Resize(40, 10));
    assert!(
        real.app
            .form
            .as_ref()
            .unwrap()
            .preview_display_v1
            .geometry_rejected
    );
    // A queued Confirm must be refused before even looking for the original
    // draft client. No worker/fence/SQL/Git/GitHub effects have been admitted.
    native::capture_form(&mut real.app);
    let form = real.app.form.as_ref().unwrap();
    assert!(!form.submit_requested && !form.submitting);
    assert!(form.error.contains("fresh useful whole FILE Preview"));
    assert!(original.matches_form(form));
    assert!(real.app.scoped_threads_work_v1.is_none());
    assert!(real.app.scoped_threads_owner_v1.is_none());
    let tiny = paint(&mut real.app, 40, 10);
    assert!(text(&tiny).contains("[Esc Close]"));
    assert!(!text(&tiny).contains("[Confirm action]"));
    real.app.dispatch(Action::Form(FormAction::Submit));
    assert!(!real.app.form.as_ref().unwrap().submit_requested);
    native::capture_form(&mut real.app);
    assert!(real.app.scoped_threads_work_v1.is_none());
    intact(&real);

    real.app.handle_event(Event::Resize(120, 40));
    paint(&mut real.app, 120, 40);
    assert!(file::form::confirmed_request_v82(real.app.form.as_ref().unwrap()).is_ok());
    native::capture_form(&mut real.app);
    assert!(real.app.scoped_threads_work_v1.is_none());
    let writer = DraftWriter::new(database(&real.fixture));
    let client = writer.client();
    real.app.dispatch(Action::Form(FormAction::Submit));
    native::capture_form_with_drafts(&mut real.app, &database(&real.fixture), &client);
    let queued = real.app.scoped_threads_work_v1.as_ref().unwrap();
    assert_eq!(queued.phase(), native::ScopedThreadPhaseV1::Confirm);
    match &queued.intent {
        native::ThreadOperationOriginV1::Files(actual) => {
            assert!(Arc::ptr_eq(actual, &original.intent().original));
        }
        _ => panic!("Confirm must keep the same original actual Files intent"),
    }
    assert!(real.app.form.as_ref().unwrap().submitting);
    // Queue admission itself cannot execute the provider or write a draft.
    drop(writer);
    intact(&real);
}

#[test]
fn file_comments_v82_return_to_fields_and_same_body_external_edit_never_restore_preview_approval() {
    let mut real = composer(true);
    paint(&mut real.app, 120, 40);
    let approved = file::form::confirmed_request_v82(real.app.form.as_ref().unwrap()).unwrap();
    real.app.dispatch(Action::Form(FormAction::Preview));
    let form = real.app.form.as_mut().unwrap();
    assert!(!form.preview);
    assert!(file::form::confirmed_request_v82(form).is_err());
    file::form::replace_editor_result(form, FILE_BODY).unwrap();
    assert_eq!(form.fields[0].editor.text, FILE_BODY);
    assert!(!approved.matches_form(form));
    assert!(file::form::request(form).unwrap().preview().is_none());
    form.submit_requested = true;
    native::capture_form(&mut real.app);
    assert!(real.app.scoped_threads_work_v1.is_none());
    assert!(real.app.scoped_threads_owner_v1.is_none());
    assert!(!real.app.form.as_ref().unwrap().submitting);
    intact(&real);
}

#[test]
fn file_comments_v82_history_closing_snapshot_rejects_unversioned_timestamp_and_legacy_resource_drift(
) {
    for legacy in [false, true] {
        let real = real(phase_steps(members(), true));
        let actual = fresh(&real).unwrap();
        let seed = FileCommentOriginSeedV75::from_producer(
            &database(&real.fixture),
            &real.app.github.as_ref().unwrap().identity,
            &actual,
        )
        .unwrap();
        let target =
            PrWriteTarget::for_source(&seed.scope, REPO, &seed.pr_node, seed.number).unwrap();
        let write = seed.write(FILE_BODY).unwrap();
        let mut journal = PrWriteJournal::open(&database(&real.fixture)).unwrap();
        let permit = journal
            .begin_file_comment_v75(&target, &write, &seed)
            .unwrap();
        assert!(permit.marker().attempt_id > 0);
        if legacy {
            writable_fixture_sql(&real.fixture)
                .execute("DELETE FROM pr_write_file_comment_origins_v75", [])
                .unwrap();
        }
        let before = journal
            .file_comment_history_v75(
                &target,
                seed.repository_id,
                &seed.repository_node,
                seed.pr_id,
            )
            .unwrap();
        journal
            .validate_file_comment_history_v75(
                &target,
                seed.repository_id,
                &seed.repository_node,
                seed.pr_id,
                &before,
            )
            .unwrap();
        if legacy {
            writable_fixture_sql(&real.fixture)
                .execute(
                    "UPDATE pr_write_attempts SET selected_resource=?1",
                    [format!("file-comment-v75/{}", "e".repeat(64))],
                )
                .unwrap();
        } else {
            writable_fixture_sql(&real.fixture)
                .execute("UPDATE pr_write_attempts SET created_at=created_at+1", [])
                .unwrap();
        }
        let after = journal
            .file_comment_history_v75(
                &target,
                seed.repository_id,
                &seed.repository_node,
                seed.pr_id,
            )
            .unwrap();
        assert_eq!(after.generation, before.generation);
        assert!(journal
            .validate_file_comment_history_v75(
                &target,
                seed.repository_id,
                &seed.repository_node,
                seed.pr_id,
                &before,
            )
            .is_err());
        assert_eq!(
            sql(&real.fixture)
                .query_row("SELECT state FROM pr_write_attempts", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "pending"
        );
        exact_protocol(&real, 0);
    }
}

#[test]
fn file_comments_v82_history_rejects_oversized_separate_repository_node_before_loading_origin_text()
{
    let real = real(phase_steps(members(), true));
    let actual = fresh(&real).unwrap();
    let seed = FileCommentOriginSeedV75::from_producer(
        &database(&real.fixture),
        &real.app.github.as_ref().unwrap().identity,
        &actual,
    )
    .unwrap();
    let target = PrWriteTarget::for_source(&seed.scope, REPO, &seed.pr_node, seed.number).unwrap();
    let mut journal = PrWriteJournal::open(&database(&real.fixture)).unwrap();
    let permit = journal
        .begin_file_comment_v75(&target, &seed.write(FILE_BODY).unwrap(), &seed)
        .unwrap();
    let db = writable_fixture_sql(&real.fixture);
    db.execute_batch("DROP TRIGGER pr_write_file_comment_origins_immutable_v75")
        .unwrap();
    db.execute(
        "UPDATE pr_write_file_comment_origins_v75 SET repository_node=?1,origin_json='deliberately invalid origin text must not be decoded' WHERE attempt_id=?2",
        rusqlite::params!["n".repeat(1025), permit.marker().attempt_id],
    )
    .unwrap();
    let result = journal.file_comment_history_v75(
        &target,
        seed.repository_id,
        &seed.repository_node,
        seed.pr_id,
    );
    let error = match result {
        Ok(_) => panic!("Oversized separate origin scalar must not be materialized/decoded"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("origin IDs/bound differ"));
    assert_eq!(
        db.query_row(
            "SELECT length(CAST(repository_node AS BLOB)) FROM pr_write_file_comment_origins_v75",
            [],
            |row| row.get::<_, usize>(0)
        )
        .unwrap(),
        1025
    );
    assert_eq!(
        db.query_row("SELECT state FROM pr_write_attempts", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "pending"
    );
    exact_protocol(&real, 0);
}
