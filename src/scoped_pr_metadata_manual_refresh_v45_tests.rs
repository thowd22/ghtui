//! Actual typed Metadata publisher -> retained Return -> same original Core.
//! A cached row is navigation only; both source families use real factories.
use super::*;
use crate::personal_pr_entry_v13::tests::collaboration_v17::protocol::publication::refresh_v19 as evidence;
use crate::{
    app::{Action, Overlay, View},
    github_workspace::GithubAction,
    runtime::{scoped_inline_test_support_v1 as channel, Background},
    scoped_pr_collaboration_native_v17 as native,
    scoped_pr_collaboration_refresh_v19::return_v19,
};
#[path = "scoped_pr_effective_cutoff_v51_tests.rs"]
mod effective_cutoff_v51;
#[path = "scoped_pr_metadata_tab_vertical_v51_tests.rs"]
mod metadata_tab_v51;

fn read_phase(field: Field, changed: bool) -> Vec<GhStep> {
    let mut calls = principal();
    calls.push(get(&format!("repos/{REPO}"), repository()));
    calls.push(get(
        &format!("repos/{REPO}/pulls/7"),
        full_raw(field, changed),
    ));
    calls.extend(visibility(field)[3..8].iter().cloned());
    calls.push(get(
        &format!("repos/{REPO}/pulls/7"),
        full_raw(field, changed),
    ));
    calls.push(get(&format!("repos/{REPO}"), repository()));
    calls.extend(principal());
    assert_eq!(calls.len(), 13);
    calls
}
fn catalogue_steps() -> Vec<GhStep> {
    let owner = json!({"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"});
    let mut calls = principal();
    calls.push(get("orgs/example", owner.clone()));
    calls.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([repository()]),
    ));
    calls.push(get("orgs/example", owner));
    calls.extend(principal());
    assert_eq!(calls.len(), 7);
    calls
}
fn install_actual_catalogue(s: &mut Setup, field: Field) {
    use crate::providers::gh::{RepositoryCatalogueLimits, RepositoryCatalogueSource};
    s.app.dispatch(Action::Github(GithubAction::Back));
    assert!(s.app.github.as_ref().unwrap().pr.is_none());
    let cancel = CancellationToken::default();
    let mut cache = Cache::open(&database(&s.fixture)).unwrap();
    let source = Arc::new(
        crate::repository_catalogue_save_v1::load_catalogue_intent_source(
            &s.api,
            &mut cache,
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &RepositoryCatalogueLimits::default(),
            &cancel,
        )
        .unwrap(),
    );
    let active = Arc::new(
        crate::active_remote_scope_v1::ActiveRemoteScopeV1::explicit_selection(
            source.source(),
            &[10000],
        )
        .unwrap(),
    );
    let previous = s.app.github.take().unwrap();
    let selected = previous.selected.clone().unwrap();
    let read = crate::scoped_pr_read_v1::open(
        source.clone(),
        active.clone(),
        previous,
        &selected,
        &s.api,
        &database(&s.fixture),
        &cancel,
    )
    .unwrap();
    assert_eq!(
        read.workspace.pr.as_ref().unwrap().details.summary.pr.body,
        CURRENT
    );
    assert!(read.workspace.scoped_pr_navigation.is_some());
    assert_eq!(
        read.workspace.pr.as_ref().unwrap().tab,
        crate::github_workspace::PrTab::Summary
    );
    s.app.context = read.workspace.context();
    s.app.github = Some(read.workspace);
    s.app.remote_scope = Some(active);
    s.app.remote_scope_source = Some(source);
    s.app.selected = 0;
    s.app.scroll = 0;
    assert!(s.app.personal_pr_source_v13.is_none());
    assert_eq!(field, Field::Title);
}
fn settle(s: &mut Setup, background: &mut Background) {
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Actual Metadata Return/Refresh worker did not settle"
        );
        std::thread::yield_now();
    }
    assert!(channel::generation(background).is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
}
fn writable(s: &Setup) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap()
}
fn protected(s: &Setup) -> Vec<String> {
    evidence::exact_sql(s)
        .into_iter()
        .filter(|row| {
            row.starts_with("pr_write_")
                || row.starts_with("drafts:")
                || row.starts_with("sync_mutation_fences:")
        })
        .collect()
}
fn returned(catalogue: bool, later: Vec<GhStep>) -> (Setup, Background, DraftWriter, Field) {
    let (s, background, writer, field, _) = returned_with_issuer_v51(catalogue, later, None);
    (s, background, writer, field)
}
fn returned_with_issuer_v51(
    catalogue: bool,
    later: Vec<GhStep>,
    issuer_lifetime: Option<Duration>,
) -> (Setup, Background, DraftWriter, Field, Option<Instant>) {
    let field = if catalogue {
        Field::Title
    } else {
        Field::Description
    };
    let mut calls = Vec::new();
    if catalogue {
        calls.extend(catalogue_steps());
        calls.extend(read_phase(field, false));
    }
    calls.extend(phase(full_raw(field, false), capability(json!(true))));
    calls.extend(phase(full_raw(field, false), capability(json!(true))));
    let publication = paid(field); // Same final UID must succeed before cleanup.
    calls.extend(publication);
    calls.extend(later);
    let mut s = setup(calls);
    if catalogue {
        install_actual_catalogue(&mut s, field);
    }
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    assert_eq!(original.catalogue_source().is_some(), catalogue);
    let mut form = metadata::open(
        original.clone(),
        field,
        &bounded(&s, &original, 1024 * 1024),
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    ui::replace_editor_result(&mut form, REQUESTED).unwrap();
    let request = metadata::request(&form).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let preview = metadata::MetadataPreviewV25::prepare(
        &request,
        &provider,
        &database(&s.fixture),
        &cancel,
        deadline,
    )
    .unwrap();
    assert!(metadata::accept_preview(&mut form, &request, preview));
    let mut writer = DraftWriter::new(database(&s.fixture));
    writer
        .save(
            form.scope(),
            form.resource(),
            ui::encoded_draft(&form).unwrap(),
        )
        .unwrap();
    writable(&s).execute_batch("CREATE TRIGGER retain_metadata_return_v45 BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'metadata-text-v25/%' BEGIN SELECT RAISE(ABORT,'retain exact metadata Return original'); END;").unwrap();
    s.app.form = Some(form);
    s.app.overlay = Some(Overlay::Form);
    s.app.form_dismissed = false;
    paint_form_v44(&mut s.app, 120, 40);
    // Second real paint enables the current exact Preview's Confirm hit.
    paint_form_v44(&mut s.app, 120, 40);
    confirm_input_v44(&mut s.app, !catalogue);
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert_eq!(work.phase(), native::ConversationPhaseKindV17::Confirm);
    let mut background = Background::new();
    let issuer = issuer_lifetime.map(|lifetime| Instant::now() + lifetime);
    let worker_provider = issuer
        .map_or_else(
            || Ok(s.api.clone()),
            |cutoff| {
                s.api
                    .clone()
                    .with_original_pr_read_bounds_v13(cutoff, 1024 * 1024)
            },
        )
        .unwrap();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        worker_provider,
        database(&s.fixture),
        writer.client()
    ));
    settle(&mut s, &mut background);
    assert!(s.app.github.is_none());
    assert!(s.app.scoped_pr_collaboration_return_v19.is_some());
    assert!(ui::return_descriptor_v19(s.app.form.as_ref().unwrap()).is_some());
    let db = writable(&s);
    assert_eq!(
        db.query_row("SELECT state FROM pr_write_attempts", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "confirmed"
    );
    assert_eq!(
        db.query_row("SELECT kind FROM sync_mutation_fences", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "pull_request_body"
    );
    assert_eq!(
        db.query_row(
            "SELECT mutation_ordinal FROM sync_mutation_fences",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        db.query_row("SELECT attempt_id FROM pr_write_attempts", [], |r| r
            .get::<_, i64>(0))
            .unwrap()
    );
    db.execute_batch("DROP TRIGGER retain_metadata_return_v45")
        .unwrap();
    paint_form_v44(&mut s.app, 120, 40);
    assert!(return_v19::capture_request(&s.app).is_ok());
    (s, background, writer, field, issuer)
}
fn submit_read(s: &mut Setup, background: &mut Background, writer: &DraftWriter) {
    submit_read_with_issuer_v51(s, background, writer, None)
}
fn submit_read_with_issuer_v51(
    s: &mut Setup,
    background: &mut Background,
    writer: &DraftWriter,
    issuer: Option<Instant>,
) {
    s.app
        .dispatch(Action::Form(FormAction::RefreshConversationV19));
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert_eq!(
        work.phase(),
        native::ConversationPhaseKindV17::RefreshExactV19
    );
    assert!(work.matches(&s.app));
    let provider = issuer
        .map_or_else(
            || Ok(s.api.clone()),
            |cutoff| {
                s.api
                    .clone()
                    .with_original_pr_read_bounds_v13(cutoff, 1024 * 1024)
            },
        )
        .unwrap();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        provider,
        database(&s.fixture),
        writer.client()
    ));
}

#[test]
fn pr_metadata_original_v25_manual_refresh_actual_catalogue_title_and_personal_description_keep_protection_and_publish_new_page(
) {
    for catalogue in [true, false] {
        let field = if catalogue {
            Field::Title
        } else {
            Field::Description
        };
        let mut calls = read_phase(field, true);
        calls.extend(phase(full_raw(field, true), capability(json!(true))));
        let (mut s, mut background, writer, _) = returned(catalogue, calls);
        let saved = protected(&s);
        let old = return_v19::capture_request(&s.app)
            .unwrap()
            .original()
            .clone();
        submit_read(&mut s, &mut background, &writer);
        settle(&mut s, &mut background);
        assert!(s.app.form.is_none() && s.app.overlay.is_none());
        assert!(s.app.scoped_pr_collaboration_return_v19.is_none());
        let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
        assert_eq!(
            if field == Field::Title {
                page.details.summary.pr.title.as_str()
            } else {
                page.details.summary.pr.body.as_str()
            },
            REQUESTED
        );
        assert_eq!(protected(&s), saved);
        assert!(!old.owns_presentation(&s.app));
        // Normal next action needs another genuine Core; the saved protected
        // envelope is still inspect-only, never restored Preview approval.
        let next = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
        let reopened = metadata::open(
            next.clone(),
            field,
            &bounded(&s, &next, 1024 * 1024),
            &database(&s.fixture),
            next.cancellation(),
            next.deadline(),
        )
        .unwrap();
        assert!(reopened.publication.is_some());
        assert!(metadata::request(&reopened).is_err());
        assert_eq!(protected(&s), saved);
        exact(&s, s.steps.len());
    }
}

#[test]
fn pr_metadata_original_v25_manual_refresh_missing_moved_and_opposite_fence_ordinals_refuse_zero_children(
) {
    for catalogue in [true, false] {
        for sql in [
            "DELETE FROM sync_mutation_fences WHERE kind='pull_request_body'",
            "UPDATE sync_mutation_fences SET mutation_ordinal=mutation_ordinal+1 WHERE kind='pull_request_body'",
            "UPDATE sync_mutation_fences SET kind='conversation_comment' WHERE kind='pull_request_body'",
            "UPDATE sync_mutation_fences SET login='user-id:43' WHERE kind='pull_request_body'",
        ] {
            let (mut s, mut background, writer, _) = returned(catalogue, vec![]);
            assert_eq!(writable(&s).execute(sql, []).unwrap(), 1);
            let before = evidence::exact_sql(&s);
            submit_read(&mut s, &mut background, &writer);
            settle(&mut s, &mut background);
            assert!(s.app.github.is_none());
            assert!(s.app.scoped_pr_collaboration_return_v19.is_some());
            assert_eq!(evidence::exact_sql(&s), before);
            exact(&s, s.steps.len());
        }
    }
}

#[test]
fn pr_metadata_original_v25_manual_refresh_personal_repeated_failed_reservations_use_actual_last_witness(
) {
    let field = Field::Description;
    let mut calls = Vec::new();
    for index in [4, 9] {
        let mut failed = read_phase(field, true);
        failed[index].status = 1;
        failed[index].stdout.clear();
        failed[index].stderr = b"Actual bounded metadata fresh read unavailable".to_vec();
        failed.truncate(index + 1);
        calls.extend(failed);
    }
    calls.extend(read_phase(field, true));
    let (mut s, mut background, writer, _) = returned(false, calls);
    let before = protected(&s);
    for _ in 0..2 {
        submit_read(&mut s, &mut background, &writer);
        settle(&mut s, &mut background);
        assert!(s.app.github.is_none());
        paint_form_v44(&mut s.app, 120, 40);
        assert!(return_v19::capture_request(&s.app).is_ok());
        assert_eq!(protected(&s), before);
    }
    submit_read(&mut s, &mut background, &writer);
    settle(&mut s, &mut background);
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .body,
        REQUESTED
    );
    assert_eq!(protected(&s), before);
    exact(&s, s.steps.len());
}

fn corrupt_origin(s: &Setup, update: &str) {
    let mut db = writable(s);
    const TRIGGER: &str = "pr_write_metadata_origins_immutable_v25";
    let schema: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name=?1",
            [TRIGGER],
            |r| r.get(0),
        )
        .unwrap();
    let before = evidence::exact_sql(s);
    let error = db.execute(update, []).unwrap_err();
    assert!(
        matches!(error, rusqlite::Error::SqliteFailure(ref e, Some(ref message)) if e.extended_code==1811 && message=="Immutable original metadata")
    );
    assert_eq!(evidence::exact_sql(s), before);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("DROP TRIGGER pr_write_metadata_origins_immutable_v25")
        .unwrap();
    assert_eq!(tx.execute(update, []).unwrap(), 1);
    tx.execute_batch(&schema).unwrap();
    assert_eq!(
        tx.query_row(
            "SELECT sql FROM sqlite_master WHERE name=?1",
            [TRIGGER],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        schema
    );
    tx.commit().unwrap();
    assert_eq!(
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE name=?1",
            [TRIGGER],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        schema
    );
}

#[test]
fn pr_metadata_original_v25_manual_refresh_held_final_uid_receipt_origin_created_ordinal_and_clear_abort_atomic_rows(
) {
    for changed in ["receipt", "origin", "created", "ordinal", "clear"] {
        let mut calls = read_phase(Field::Description, true);
        calls[12].delay_ms = 2000;
        let (s, _background, _writer, _) = returned(false, calls);
        let request = return_v19::capture_request(&s.app).unwrap();
        let cancel = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        let provider = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
            .unwrap();
        std::thread::scope(|scope| {
            let worker = scope
                .spawn(|| request.execute(&provider, &database(&s.fixture), &cancel, deadline));
            let held = evidence::StoppedV19::final_uid(&s);
            match changed {
                "receipt" => assert_eq!(writable(&s).execute("UPDATE pr_write_attempts SET confirmed_json=json_set(confirmed_json,'$.resource_id','ACTUAL_DIFFERENT_RECEIPT')", []).unwrap(), 1),
                "origin" => corrupt_origin(&s, "UPDATE pr_write_metadata_origins_v25 SET origin_json=origin_json || ' '"),
                "created" => assert_eq!(writable(&s).execute("UPDATE pr_write_attempts SET created_at=created_at+1", []).unwrap(), 1),
                "ordinal" => assert_eq!(writable(&s).execute("UPDATE sync_mutation_fences SET mutation_ordinal=mutation_ordinal+1 WHERE kind='pull_request_body'", []).unwrap(), 1),
                "clear" => { Cache::open(&database(&s.fixture)).unwrap().clear_records(&Scope::github("github.invalid", Some(42)).unwrap()).unwrap(); },
                _ => unreachable!(),
            }
            let expected = evidence::exact_sql(&s);
            drop(held);
            assert!(worker.join().unwrap().is_err());
            assert_eq!(
                evidence::exact_sql(&s),
                expected,
                "No raw-CAS drift is repaired or partially published"
            );
        });
        assert!(s.app.github.is_none());
        exact(&s, s.steps.len());
    }
}

#[test]
fn pr_metadata_original_v25_manual_refresh_actual_final_uid_loss_hides_return_without_touching_original(
) {
    for catalogue in [true, false] {
        let field = if catalogue {
            Field::Title
        } else {
            Field::Description
        };
        let mut calls = read_phase(field, true);
        calls[12].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
        let (mut s, mut background, writer, _) = returned(catalogue, calls);
        let original = protected(&s);
        let before = sql(&s.fixture); // Every table except the real read reservation.
        let mut reservations = reservations_v44(&s);
        if !catalogue {
            let mut changed = 0;
            for row in &mut reservations {
                if matches!(&row[2], rusqlite::types::Value::Text(name) if name.starts_with("personal-pr-source-v13/"))
                {
                    let rusqlite::types::Value::Integer(generation) = &mut row[3] else {
                        panic!("Actual integer reservation")
                    };
                    *generation += 1;
                    row[4] = rusqlite::types::Value::Integer(1);
                    changed += 1;
                }
            }
            assert_eq!(
                changed, 1,
                "Only the real current Personal reservation advances"
            );
        }
        submit_read(&mut s, &mut background, &writer);
        settle(&mut s, &mut background);
        assert!(s.app.github.is_none() && s.app.form.is_none());
        assert!(s.app.scoped_pr_collaboration_return_v19.is_none());
        let text = evidence::paint(&mut s.app);
        for private in [CURRENT, REQUESTED, BODY] {
            assert!(!text.contains(private));
        }
        assert_eq!(protected(&s), original);
        assert_eq!(sql(&s.fixture), before);
        assert_eq!(reservations_v44(&s), reservations);
        exact(&s, s.steps.len());
    }
}

#[test]
fn pr_metadata_original_v25_manual_refresh_expired_and_cancelled_phases_never_renew_then_held_read_cancels(
) {
    let mut calls = read_phase(Field::Description, true);
    calls[9].delay_ms = 2000;
    calls.truncate(10);
    let (s, _background, _writer, _) = returned(false, calls);
    let request = return_v19::capture_request(&s.app).unwrap();
    let before = evidence::exact_sql(&s);
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let expired = CancellationToken::default();
    assert!(request
        .execute(
            &provider,
            &database(&s.fixture),
            &expired,
            Instant::now() - Duration::from_millis(1)
        )
        .is_err());
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(request
        .execute(&provider, &database(&s.fixture), &cancelled, deadline)
        .is_err());
    assert_eq!(evidence::exact_sql(&s), before);
    let cancel = CancellationToken::default();
    let protected_before = protected(&s);
    std::thread::scope(|scope| {
        let worker =
            scope.spawn(|| request.execute(&provider, &database(&s.fixture), &cancel, deadline));
        let held = evidence::StoppedV19::final_uid(&s);
        cancel.cancel();
        drop(held);
        assert!(worker.join().unwrap().is_err());
    });
    assert_eq!(protected(&s), protected_before);
    assert!(s.app.github.is_none());
    assert!(
        return_v19::capture_request(&s.app).is_ok(),
        "Only real settled Personal reservation witness permits a separate explicit new read"
    );
    exact(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_manual_refresh_queued_old_return_cannot_capture_fence_or_replace_new_actual_source(
) {
    let (mut s, mut background, writer, _) = returned(false, vec![]);
    s.app
        .dispatch(Action::Form(FormAction::RefreshConversationV19));
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(work.matches(&s.app));
    let mut genuine = setup(vec![]);
    let page = allocations(&genuine.app);
    let new_sql = evidence::exact_sql(&genuine);
    let original_sql = evidence::exact_sql(&s);
    s.app = std::mem::take(&mut genuine.app);
    let status = s.app.status.clone();
    let loading = s.app.loading;
    assert!(!work.matches(&s.app));
    assert!(!background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    assert_eq!(allocations(&s.app), page);
    assert_eq!(s.app.status, status);
    assert_eq!(s.app.loading, loading);
    assert!(s.app.form.is_none());
    assert!(s.app.scoped_pr_collaboration_return_v19.is_none());
    assert!(channel::generation(&background).is_none());
    assert_eq!(evidence::exact_sql(&s), original_sql);
    assert_eq!(evidence::exact_sql(&genuine), new_sql);
    exact(&s, s.steps.len());
    exact(&genuine, genuine.steps.len());
}

#[test]
fn pr_metadata_original_v25_manual_refresh_narrow_issuer_cap_cannot_be_widened_by_new_read() {
    for catalogue in [true, false] {
        let field = if catalogue {
            Field::Title
        } else {
            Field::Description
        };
        let calls = read_phase(field, true)[..3].to_vec();
        let (s, _background, _writer, _) = returned(catalogue, calls);
        let request = return_v19::capture_request(&s.app).unwrap();
        let before = evidence::exact_sql(&s);
        let cancel = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        let provider = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 64)
            .unwrap()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
            .unwrap();
        assert!(request
            .execute(&provider, &database(&s.fixture), &cancel, deadline)
            .is_err());
        assert_eq!(
            evidence::exact_sql(&s),
            before,
            "Oversized actual repository child cannot reserve or publish anything"
        );
        assert!(s.app.github.is_none());
        exact(&s, s.steps.len());
    }
}
