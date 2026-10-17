//! Genuine normal Open/Preview factories, actual draft SQL and SAME Background.
//! The accepted Form is MOVED, never cloned or decoded into source authority.
use super::*;
use crate::{
    app::{Action, PrUiAction},
    drafts::DraftWriter,
    forms::FormAction,
    personal_pr_entry_v13::tests::{database, sql, Setup},
    pr_actions::PrCommand,
    scoped_pr_collaboration_ui_v17 as ui,
    storage::Cache,
    test_support::GhStep,
};
use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

fn installed(tail: Vec<GhStep>) -> (Setup, DraftWriter) {
    let (mut s, mut form, _) = prepared(PrCommand::NewComment, tail);
    let mut writer = DraftWriter::new(database(&s.fixture));
    let encoded = ui::encoded_draft(&form).unwrap();
    writer
        .save(form.scope(), form.resource(), encoded.clone())
        .unwrap();
    // Actual ordered durable ACK, not dirty=false as durability evidence.
    writer
        .client()
        .save_and_wait(form.scope(), form.resource(), encoded)
        .unwrap();
    form.dirty = false;
    s.app.form = Some(form);
    s.app.overlay = Some(Overlay::Form);
    (s, writer)
}
fn close(s: &mut Setup) {
    s.app.dispatch(Action::Form(FormAction::Close));
    assert!(s.app.form.is_none());
    assert!(s.app.overlay.is_none());
    assert!(s.app.closed_conversation_draft_v20.is_some());
}
fn request(s: &mut Setup) -> Arc<native::ScopedPrCollaborationWorkV17> {
    s.app
        .dispatch(Action::Pr(PrUiAction::Command(PrCommand::NewComment)));
    assert!(native::capture_action(
        &mut s.app,
        &PrUiAction::Command(PrCommand::NewComment),
        &database(&s.fixture)
    ));
    s.app.pr_action = None; // same main-loop consumption AFTER capture
    s.app
        .scoped_pr_collaboration_work_v17
        .take()
        .expect("Actual fresh normal Open")
}
fn finish_read(s: &mut Setup, background: &mut Background) {
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Actual original read did not finish"
        );
        std::thread::yield_now();
    }
}
fn commands(s: &Setup) -> Vec<u8> {
    fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap()
}

#[test]
fn conversation_comments_v20_pure_close_moves_exact_inert_form_without_sql_git_or_gh_then_genuine_open_restores(
) {
    let (mut s, writer) = installed(phase(PrCommand::NewComment, true, Some(true)));
    let presentation = s.app.form.as_ref().unwrap().presentation_v4().0.clone();
    let source = ui::request(s.app.form.as_ref().unwrap())
        .unwrap()
        .intent()
        .source
        .clone();
    let before = sql(&s.fixture);
    let gh = commands(&s);
    close(&mut s);
    assert_eq!(sql(&s.fixture), before);
    assert_eq!(commands(&s), gh);
    let work = request(&mut s);
    assert!(work.matches(&s.app));
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    finish_read(&mut s, &mut background);
    let form = s.app.form.as_ref().expect("Fresh actual Core result");
    assert_eq!(form.values()["body"], TEXT);
    assert!(!form.preview && !form.submitting && form.publication.is_none());
    assert!(!Arc::ptr_eq(form.presentation_v4().0, &presentation));
    assert!(!Arc::ptr_eq(
        &ui::request(form).unwrap().intent().source,
        &source
    ));
    assert!(s.app.closed_conversation_draft_v20.is_none());
    assert_eq!(sql(&s.fixture), before);
    exact_protocol(&s);
}

#[test]
fn conversation_comments_v20_dirty_busy_pending_editor_and_foreign_source_close_never_moves_form() {
    for mode in 0..7 {
        let (mut s, _writer) = installed(vec![]);
        let presentation = s.app.form.as_ref().unwrap().presentation_v4().0.clone();
        match mode {
            0 => s.app.form.as_mut().unwrap().dirty = true,
            1 => s.app.form.as_mut().unwrap().submitting = true,
            2 => s.app.form.as_mut().unwrap().editor_requested = true,
            3 => s.app.refresh_requested = true,
            4 => s.app.github.as_mut().unwrap().identity.account_id = Some(43),
            5 => s.app.form.as_mut().unwrap().preview_requested = true,
            _ => s.app.form.as_mut().unwrap().submit_requested = true,
        }
        let before_sql = sql(&s.fixture);
        let gh = commands(&s);
        s.app.dispatch(Action::Form(FormAction::Close));
        assert!(s.app.closed_conversation_draft_v20.is_none());
        assert!(Arc::ptr_eq(
            s.app.form.as_ref().unwrap().presentation_v4().0,
            &presentation
        ));
        assert_eq!(sql(&s.fixture), before_sql);
        assert_eq!(commands(&s), gh);
    }
}

#[test]
fn conversation_comments_v20_newer_notice_form_view_account_and_clear_retire_only_old_private_slot()
{
    for mode in 0..5 {
        let (mut s, _writer) = installed(vec![]);
        close(&mut s);
        let gh = commands(&s);
        match mode {
            0 => {
                s.app.overlay = Some(Overlay::Notice);
                s.app.notice = "INDEPENDENT_CURRENT_NOTICE".into();
                // A Notice prevents restore; its UI is not cleared by retirement.
                s.app.github.as_mut().unwrap().identity.account_id = Some(43);
            }
            1 => {
                let (_, form, _) = prepared(PrCommand::NewComment, vec![]);
                s.app.form = Some(form);
                s.app.overlay = Some(Overlay::Form);
            }
            2 => {
                s.app.dispatch(Action::View(View::Repository));
            }
            3 => s.app.github.as_mut().unwrap().identity.account_id = Some(43),
            _ => crate::repository_catalogue_native::retire_remote_views(&mut s.app),
        }
        let overlay = s.app.overlay;
        let presentation = s.app.form.as_ref().map(|f| f.presentation_v4().0.clone());
        ui::pure_close_v20::retire_invalid(&mut s.app);
        assert!(s.app.closed_conversation_draft_v20.is_none());
        assert_eq!(s.app.overlay, overlay);
        if let Some(presentation) = presentation {
            assert!(Arc::ptr_eq(
                s.app.form.as_ref().unwrap().presentation_v4().0,
                &presentation
            ));
        }
        assert_eq!(commands(&s), gh);
    }
}

#[test]
fn conversation_comments_v20_actual_held_accepted_save_close_and_reopen_waits_before_private_gh() {
    let (mut s, mut writer) = installed(phase(PrCommand::NewComment, true, Some(true)));
    // Return through the genuine local Preview Back, then edit normally.
    s.app.dispatch(Action::Form(FormAction::Preview));
    ui::replace_editor_result(
        s.app.form.as_mut().unwrap(),
        "Accepted but not yet durable 界",
    )
    .unwrap();
    let sql = rusqlite::Connection::open(database(&s.fixture)).unwrap();
    sql.execute_batch("BEGIN IMMEDIATE; UPDATE drafts SET body=body;")
        .unwrap();
    let form = s.app.form.as_mut().unwrap();
    writer
        .save(
            form.scope(),
            form.resource(),
            ui::encoded_draft(form).unwrap(),
        )
        .unwrap();
    form.dirty = false; // accepted only, deliberately NOT durable
    let gh = commands(&s);
    close(&mut s);
    let work = request(&mut s);
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    assert_eq!(
        commands(&s),
        gh,
        "Open cannot overtake the actual locked accepted Save"
    );
    sql.execute_batch("COMMIT;").unwrap();
    finish_read(&mut s, &mut background);
    assert_eq!(
        s.app.form.as_ref().unwrap().values()["body"],
        "Accepted but not yet durable 界"
    );
    assert!(s.app.form.as_ref().unwrap().publication.is_none());
    assert!(s.app.closed_conversation_draft_v20.is_none());
    exact_protocol(&s);
}

#[test]
fn conversation_comments_v20_actual_failed_save_restores_only_exact_original_editable_body_no_gh_or_replay(
) {
    let (mut s, mut writer) = installed(vec![]);
    s.app.dispatch(Action::Form(FormAction::Preview));
    ui::replace_editor_result(s.app.form.as_mut().unwrap(), "UNSAVED_ORIGINAL_BODY_界").unwrap();
    let db = rusqlite::Connection::open(database(&s.fixture)).unwrap();
    db.execute_batch("CREATE TRIGGER actual_local_save_failure BEFORE INSERT ON drafts BEGIN SELECT RAISE(ABORT,'actual local save refused'); END;").unwrap();
    let form = s.app.form.as_mut().unwrap();
    let presentation = form.presentation_v4().0.clone();
    writer
        .save(
            form.scope(),
            form.resource(),
            ui::encoded_draft(form).unwrap(),
        )
        .unwrap();
    form.dirty = false;
    let before_sql = sql(&s.fixture);
    let gh = commands(&s);
    close(&mut s);
    let work = request(&mut s);
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    finish_read(&mut s, &mut background);
    let form = s
        .app
        .form
        .as_ref()
        .expect("Typed local failure restores exact retained Form");
    assert!(Arc::ptr_eq(form.presentation_v4().0, &presentation));
    assert_eq!(form.values()["body"], "UNSAVED_ORIGINAL_BODY_界");
    assert!(!form.preview && !form.dirty && !form.submitting && form.publication.is_none());
    assert!(
        form.error.contains("Local draft save failed") && form.error.contains("edit explicitly")
    );
    assert_eq!(commands(&s), gh);
    assert_eq!(sql(&s.fixture), before_sql);
    assert!(ui::confirmed_preview(form).is_err());
}

#[test]
fn conversation_comments_v20_failed_cutoff_newer_form_or_notice_cannot_restore_or_clear_ui() {
    for notice in [false, true] {
        let (mut s, _writer) = installed(vec![]);
        close(&mut s);
        let ticket = ui::pure_close_v20::capture_ticket(&s.app, PrCommand::NewComment).unwrap();
        if notice {
            s.app.overlay = Some(Overlay::Notice);
        } else {
            let (_, form, _) = prepared(PrCommand::NewComment, vec![]);
            s.app.form = Some(form);
            s.app.overlay = Some(Overlay::Form);
        }
        let overlay = s.app.overlay;
        let original = s.app.form.as_ref().map(|f| f.presentation_v4().0.clone());
        assert!(!ui::pure_close_v20::restore_failed_save(
            &mut s.app,
            Some(&ticket)
        ));
        assert_eq!(s.app.overlay, overlay);
        if let Some(original) = original {
            assert!(Arc::ptr_eq(
                s.app.form.as_ref().unwrap().presentation_v4().0,
                &original
            ));
        }
    }
}

#[test]
fn conversation_comments_v20_actual_clear_and_database_replacement_refuse_failed_save_body_restore_before_any_gh(
) {
    for replace in [false, true] {
        let (mut s, mut writer) = installed(vec![]);
        s.app.dispatch(Action::Form(FormAction::Preview));
        ui::replace_editor_result(
            s.app.form.as_mut().unwrap(),
            "PRIVATE_FAILED_BODY_MUST_NOT_REAPPEAR",
        )
        .unwrap();
        let path = database(&s.fixture);
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TRIGGER original_failed_save BEFORE INSERT ON drafts BEGIN SELECT RAISE(ABORT,'actual failed original save'); END;").unwrap();
        let form = s.app.form.as_mut().unwrap();
        let scope = form.scope();
        writer
            .save(
                scope.clone(),
                form.resource(),
                ui::encoded_draft(form).unwrap(),
            )
            .unwrap();
        form.dirty = false;
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            let acks = writer.poll();
            if acks.iter().any(|ack| ack.result.is_err()) {
                break;
            }
            assert!(Instant::now() < until, "Actual failed Save did not ACK");
            std::thread::yield_now();
        }
        close(&mut s);
        drop(db);
        if replace {
            fs::rename(&path, s.fixture.root.join("retained-original.sqlite3")).unwrap();
            drop(Cache::open(&path).unwrap());
        } else {
            Cache::open(&path)
                .unwrap()
                .clear_scope(&scope, false)
                .unwrap();
        }
        let before_sql = sql(&s.fixture);
        let gh = commands(&s);
        let work = request(&mut s);
        let mut background = Background::new();
        assert!(background.submit_scoped_pr_collaboration_v17(
            &mut s.app,
            work,
            s.api.clone(),
            path,
            writer.client()
        ));
        finish_read(&mut s, &mut background);
        assert!(
            s.app.form.is_none(),
            "Actual old local boundary cannot restore private body"
        );
        assert!(s.app.closed_conversation_draft_v20.is_none());
        assert!(
            s.app.github.is_none(),
            "Same original private source retires on real boundary loss"
        );
        assert_eq!(commands(&s), gh);
        assert_eq!(sql(&s.fixture), before_sql);
    }
}

#[test]
fn conversation_comments_v20_actual_held_failed_save_and_newer_genuine_form_never_publish_old_body_or_status(
) {
    let (mut s, mut writer) = installed(vec![]);
    s.app.dispatch(Action::Form(FormAction::Preview));
    ui::replace_editor_result(s.app.form.as_mut().unwrap(), "OLD_FAILED_PRIVATE_BODY").unwrap();
    let path = database(&s.fixture);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_held_save BEFORE INSERT ON drafts BEGIN SELECT RAISE(ABORT,'actual held failed save'); END; BEGIN IMMEDIATE; UPDATE drafts SET body=body;").unwrap();
    let form = s.app.form.as_mut().unwrap();
    writer
        .save(
            form.scope(),
            form.resource(),
            ui::encoded_draft(form).unwrap(),
        )
        .unwrap();
    form.dirty = false;
    close(&mut s);
    let work = request(&mut s);
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        path,
        writer.client()
    ));
    let old_owner = s.app.scoped_pr_collaboration_owner_v17.clone().unwrap();
    let (_, current_form, _) = prepared(PrCommand::NewComment, vec![]);
    let current_presentation = current_form.presentation_v4().0.clone();
    s.app.form = Some(current_form);
    s.app.overlay = Some(Overlay::Form);
    s.app.form_dismissed = false;
    s.app.status = "GENUINE_NEWER_FORM_STATUS".into();
    let gh = commands(&s);
    background.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
    assert!(old_owner.cancellation().is_cancelled());
    assert!(s.app.closed_conversation_draft_v20.is_none());
    db.execute_batch("COMMIT;").unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        background.poll(&mut s.app);
        if writer.poll().iter().any(|ack| ack.result.is_err()) {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    background.poll(&mut s.app);
    assert_eq!(s.app.status, "GENUINE_NEWER_FORM_STATUS");
    assert_eq!(s.app.overlay, Some(Overlay::Form));
    assert!(Arc::ptr_eq(
        s.app.form.as_ref().unwrap().presentation_v4().0,
        &current_presentation
    ));
    assert!(!rendered(&mut s.app).contains("OLD_FAILED_PRIVATE_BODY"));
    assert_eq!(commands(&s), gh);
}
