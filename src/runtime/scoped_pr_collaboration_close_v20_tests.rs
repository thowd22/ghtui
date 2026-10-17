//! Real factories and original channel faults; no fabricated source/owner.
use super::*;
use crate::{
    app::{Action, Overlay, PrUiAction},
    drafts::DraftWriter,
    forms::FormAction,
    personal_pr_entry_v13::tests as personal,
    personal_pr_entry_v13::tests::collaboration_v17::protocol::publication,
    pr_actions::PrCommand,
    scoped_pr_collaboration_ui_v17 as ui,
};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn conversation_comments_v20_full_disconnected_absent_queue_preserve_body_and_original_publication_ticket(
) {
    for mode in 0..3 {
        let (mut s, mut form, _) = publication::prepared(PrCommand::NewComment, vec![]);
        let path = personal::database(&s.fixture);
        let mut writer = DraftWriter::new(path.clone());
        let encoded = ui::encoded_draft(&form).unwrap();
        writer
            .save(form.scope(), form.resource(), encoded.clone())
            .unwrap();
        writer
            .client()
            .save_and_wait(form.scope(), form.resource(), encoded.clone())
            .unwrap();
        form.dirty = false;
        s.app.form = Some(form);
        s.app.overlay = Some(Overlay::Form);
        let token = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(8);
        let form = s.app.form.as_ref().unwrap();
        let pub_fence = writer
            .client()
            .capture_publication_v2(&path, &form.scope(), &form.resource(), &token, deadline)
            .unwrap();
        s.app.dispatch(Action::Form(FormAction::Close));
        assert!(s.app.form.is_none() && s.app.closed_conversation_draft_v20.is_some());
        native::capture_action(
            &mut s.app,
            &PrUiAction::Command(PrCommand::NewComment),
            &path,
        );
        let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
        let mut bg = Background::new();
        let (tx, rx) = mpsc::sync_channel(0);
        let held = match mode {
            0 => {
                bg.requests = Some(tx);
                Some(rx)
            }
            1 => {
                drop(rx);
                bg.requests = Some(tx);
                None
            }
            _ => {
                bg.requests = None;
                None
            }
        };
        let generation = bg.generation;
        let status = s.app.status.clone();
        let loading = s.app.loading;
        let before_sql = personal::sql(&s.fixture);
        let commands = std::fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        assert!(!bg.submit_scoped_pr_collaboration_v17(
            &mut s.app,
            work,
            s.api.clone(),
            path.clone(),
            writer.client()
        ));
        assert_eq!(bg.generation, generation);
        assert!(bg.current.is_empty() && bg.current_collaboration_v17.is_none());
        assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
        assert_eq!(s.app.loading, loading);
        assert_eq!(s.app.status, status);
        assert!(s.app.form.is_none() && s.app.closed_conversation_draft_v20.is_some());
        assert_eq!(personal::sql(&s.fixture), before_sql);
        assert_eq!(
            std::fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
            commands
        );
        // Failed admission dropped ONLY its own read ticket. No older genuine
        // publication capture was superseded, nor any body Save replayed.
        let read = writer
            .client()
            .capture_conversation_open_cutoff_v20(&path, &token, deadline)
            .unwrap();
        writer
            .client()
            .wait_conversation_open_cutoff_v20(&read, &token)
            .unwrap();
        drop(read);
        writer
            .client()
            .save_publication_and_wait_v2(pub_fence, encoded, &token)
            .unwrap();
        drop(held);
    }
}

#[test]
fn conversation_comments_v20_occupied_original_read_ticket_refuses_new_open_before_any_slot_or_form_effect(
) {
    let (mut s, mut form, _) = publication::prepared(PrCommand::NewComment, vec![]);
    let path = personal::database(&s.fixture);
    let writer = DraftWriter::new(path.clone());
    form.dirty = false;
    s.app.form = Some(form);
    s.app.overlay = Some(Overlay::Form);
    s.app.dispatch(Action::Form(FormAction::Close));
    let token = CancellationToken::default();
    let cutoff = writer
        .client()
        .capture_conversation_open_cutoff_v20(
            &path,
            &token,
            Instant::now() + Duration::from_secs(8),
        )
        .unwrap();
    native::capture_action(
        &mut s.app,
        &PrUiAction::Command(PrCommand::NewComment),
        &path,
    );
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    let mut bg = Background::new();
    let generation = bg.generation;
    let status = s.app.status.clone();
    let before_sql = personal::sql(&s.fixture);
    assert!(!bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        path.clone(),
        writer.client()
    ));
    assert_eq!(bg.generation, generation);
    assert!(bg.current.is_empty() && !s.app.loading[View::Inbox.index()]);
    assert!(s.app.closed_conversation_draft_v20.is_some() && s.app.form.is_none());
    assert_eq!(s.app.status, status);
    assert_eq!(personal::sql(&s.fixture), before_sql);
    cutoff.check(&writer.client(), &token).unwrap();
}
