//! Actual factories -> the SAME two Main autosave methods -> real DraftWriter.
//! Queue acceptance never substitutes for the separately observed SQLite ACK.
use super::*;
use crossterm::event::{KeyCode, KeyModifiers};

fn selected(catalogue: bool) -> (Setup, Form, Arc<member_ui::MetadataMembersPreviewV48>) {
    prepared_metadata(Cmd::AddLabel, catalogue, Vec::new())
}
fn assert_no_write(s: &Setup) {
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        0
    );
    exact(s, s.steps.len());
}
fn assert_preview(form: &Form, preview: &Arc<member_ui::MetadataMembersPreviewV48>) {
    assert!(form.preview);
    assert!(Arc::ptr_eq(
        member_ui::state(form).unwrap().preview.as_ref().unwrap(),
        preview
    ));
    assert!(member_ui::request(form).unwrap().matches_confirm(form));
}
fn wait_ack(writer: &mut DraftWriter, form: &Form, body: &str, s: &Setup) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let acks = writer.poll();
        if !acks.is_empty() {
            assert_eq!(acks.len(), 1);
            let ack = acks.into_iter().next().unwrap();
            assert_eq!(ack.scope, form.scope());
            assert_eq!(ack.resource, form.resource());
            ack.result.unwrap();
            break;
        }
        assert!(Instant::now() < until, "Actual exact initial draft ACK");
        std::thread::yield_now();
    }
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
            .unwrap()
            .as_deref(),
        Some(body)
    );
}

#[test]
fn pr_metadata_members_v77_main_initial_save_keeps_exact_preview_through_real_sqlite_ack_both_origins(
) {
    for catalogue in [false, true] {
        let (s, mut form, preview) = selected(catalogue);
        assert!(member_ui::pending_initial_save_v77(&form));
        let epoch = form.presentation_v4().1;
        let revision = member_ui::state(&form).unwrap().revision;
        let mut writer = DraftWriter::new(database(&s.fixture));
        form.prepare_draft_autosave_v77();
        assert_eq!(form.presentation_v4().1, epoch);
        assert_eq!(member_ui::state(&form).unwrap().revision, revision);
        let body = ui::encoded_draft(&form).unwrap();
        let accepted = writer.save(form.scope(), form.resource(), body.clone());
        form.settle_draft_autosave_v77(accepted.is_ok());
        accepted.unwrap();
        assert!(!form.dirty);
        assert!(!member_ui::pending_initial_save_v77(&form));
        assert_preview(&form, &preview);
        // No assumption that accepted means saved: observe the actual writer
        // ACK and read the exact SQLite bytes before making that claim.
        wait_ack(&mut writer, &form, &body, &s);
        assert_preview(&form, &preview);
        let client = writer.client();
        let cancel = CancellationToken::default();
        let fence = client
            .capture_publication_v2(
                &database(&s.fixture),
                &form.scope(),
                &form.resource(),
                &cancel,
                Instant::now() + Duration::from_secs(8),
            )
            .unwrap();
        let ack = client
            .save_publication_and_wait_v2(fence, body.clone(), &cancel)
            .unwrap();
        ack.check_current(
            &client,
            &database(&s.fixture),
            &form.scope(),
            &form.resource(),
            &cancel,
        )
        .unwrap();
        assert_no_write(&s);
    }
}

#[test]
fn pr_metadata_members_v77_initial_save_rejects_changed_values_epoch_foreign_form_and_real_invalidation(
) {
    for catalogue in [false, true] {
        for fault in 0..4 {
            let (s, mut form, preview) = selected(catalogue);
            let original_body = form.fields[0].editor.text.clone();
            match fault {
                0 => {
                    form.fields[0].editor =
                        crate::editor::TextBuffer::new("different raw selected value").unwrap()
                }
                1 => {
                    let _ = form.key(KeyCode::Tab, KeyModifiers::NONE);
                }
                2 => {
                    let mut foreign = Form::pr(crate::forms::PrIntent {
                        kind: crate::forms::PrFormKind::Comment,
                        target: preview.intent.source.parent().summary().clone(),
                        identity: preview.identity.clone(),
                    })
                    .unwrap();
                    foreign.purpose = form.purpose.clone();
                    foreign.fields = form.fields.clone();
                    foreign.dirty = true;
                    foreign.preview = true;
                    form = foreign;
                }
                3 => form.invalidate_plan(),
                _ => unreachable!(),
            }
            assert!(!member_ui::pending_initial_save_v77(&form));
            let bytes = form.fields[0].editor.text.clone();
            form.prepare_draft_autosave_v77();
            assert!(!form.preview);
            assert!(member_ui::state(&form).unwrap().preview.is_none());
            assert_eq!(form.fields[0].editor.text, bytes);
            if fault != 0 {
                assert_eq!(bytes, original_body);
            }
            assert!(!member_ui::pending_initial_save_v77(&form));
            assert!(Cache::open(&database(&s.fixture))
                .unwrap()
                .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
                .unwrap()
                .is_none());
            assert_no_write(&s);
        }
    }
}

#[test]
fn pr_metadata_members_v77_actual_full_writer_queue_retires_preview_keeps_body_and_dirty_both_origins(
) {
    for catalogue in [false, true] {
        let (s, mut form, _preview) = selected(catalogue);
        let mut writer = DraftWriter::new(database(&s.fixture));
        writer
            .client()
            .save_and_wait(form.scope(), "v77-initialized".into(), "initialized".into())
            .unwrap();
        let held = rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        held.execute_batch("BEGIN IMMEDIATE").unwrap();
        // Actual held SQLite writer: eight queued + sixteen coalesced + one
        // writing. Wait for the STRUCTURAL25 acceptance, not a timed sleep.
        let mut accepted = 0;
        let until = Instant::now() + Duration::from_secs(8);
        while accepted < 25 {
            match writer.save(
                form.scope(),
                format!("v77-older-{accepted}"),
                format!("older body {accepted}"),
            ) {
                Ok(()) => accepted += 1,
                Err(error) => {
                    assert!(error.contains("Draft queue is full"));
                    std::thread::yield_now();
                }
            }
            assert!(
                Instant::now() < until,
                "Real bounded writer queue must become occupied"
            );
        }
        assert!(member_ui::pending_initial_save_v77(&form));
        let raw = form.fields[0].editor.text.clone();
        form.prepare_draft_autosave_v77();
        let encoded = ui::encoded_draft(&form).unwrap();
        let failure = writer.save(form.scope(), form.resource(), encoded);
        assert!(failure
            .as_ref()
            .unwrap_err()
            .contains("Draft queue is full"));
        form.settle_draft_autosave_v77(failure.is_ok());
        form.error = format!("Draft error: {}", failure.unwrap_err());
        assert!(form.error.contains("Draft error: Draft queue is full"));
        assert!(form.dirty);
        assert!(!form.preview);
        assert!(member_ui::state(&form).unwrap().preview.is_none());
        assert!(!member_ui::pending_initial_save_v77(&form));
        assert_eq!(form.fields[0].editor.text, raw);
        held.execute_batch("ROLLBACK").unwrap();
        let mut acknowledged = std::collections::BTreeSet::new();
        let until = Instant::now() + Duration::from_secs(8);
        while acknowledged.len() < 25 {
            for ack in writer.poll() {
                assert_eq!(ack.scope, form.scope());
                assert!(ack.resource.starts_with("v77-older-"));
                ack.result.unwrap();
                assert!(acknowledged.insert(ack.resource));
            }
            assert!(
                Instant::now() < until,
                "Drain only actual accepted local saves"
            );
            std::thread::yield_now();
        }
        let cache = Cache::open(&database(&s.fixture)).unwrap();
        assert!(cache
            .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
            .unwrap()
            .is_none());
        for index in 0..25 {
            assert_eq!(
                cache
                    .bounded_draft(&form.scope(), &format!("v77-older-{index}"), 128 * 1024)
                    .unwrap(),
                Some(format!("older body {index}"))
            );
        }
        assert_no_write(&s);
    }
}

#[test]
fn pr_metadata_members_v77_consumed_initial_save_cannot_bypass_a_later_generic_save() {
    for catalogue in [false, true] {
        let (s, mut form, preview) = selected(catalogue);
        let mut writer = DraftWriter::new(database(&s.fixture));
        form.prepare_draft_autosave_v77();
        let body = ui::encoded_draft(&form).unwrap();
        let accepted = writer.save(form.scope(), form.resource(), body.clone());
        form.settle_draft_autosave_v77(accepted.is_ok());
        accepted.unwrap();
        wait_ack(&mut writer, &form, &body, &s);
        assert_preview(&form, &preview);
        let before = form.presentation_v4().1;
        // Simulate another ordinary dirty registration, not a new producer.
        // The single token was consumed, so Main must invalidate as before.
        form.dirty = true;
        form.prepare_draft_autosave_v77();
        assert!(!form.preview);
        assert_ne!(form.presentation_v4().1, before);
        assert!(member_ui::state(&form).unwrap().preview.is_none());
        assert_eq!(form.fields[0].editor.text.as_str(), preview.values["body"]);
        assert_no_write(&s);
    }
}
