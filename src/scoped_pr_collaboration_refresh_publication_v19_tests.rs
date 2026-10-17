//! Actual same publisher/Core/SQLite and Background. Direct worker cases do
//! not prove foreground prequeue capture; the actual channel cases do use it.
use super::*;
use crate::owned_pty_processes_test_support as processes;
use crate::{
    app::{Action, Overlay, View},
    forms::FormAction,
    runtime::{scoped_inline_test_support_v1 as channel, Background},
    scoped_pr_collaboration_native_v17 as native,
};

fn writable(s: &Setup) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap()
}
/// Hostile disposable same-file edit, never an application SQL capability.
/// Restore the exact production trigger in the SAME Immediate transaction
/// before resuming the actual held final-UID child and its guarded commit.
fn corrupt_original_metadata_v19(s: &Setup, sql: &str) {
    let mut connection = writable(s);
    if !sql.starts_with("UPDATE pr_write_conversation_origins_v17 ") {
        assert_eq!(connection.execute(sql, []).unwrap(), 1);
        return;
    }
    const NAME: &str = "pr_write_conversation_origins_immutable_v17";
    let trigger: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='trigger' AND name=?1",
            [NAME],
            |row| row.get(0),
        )
        .unwrap();
    let before = exact_sql(s);
    let refusal = connection.execute(sql, []).unwrap_err();
    assert!(
        matches!(refusal, rusqlite::Error::SqliteFailure(error, Some(ref message))
        if error.extended_code == 1811 && message == "Immutable original Conversation metadata")
    );
    assert_eq!(
        exact_sql(s),
        before,
        "Immutable schema rejects ordinary edits"
    );
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    transaction
        .execute_batch("DROP TRIGGER pr_write_conversation_origins_immutable_v17")
        .unwrap();
    assert_eq!(transaction.execute(sql, []).unwrap(), 1);
    transaction.execute_batch(&trigger).unwrap();
    let restored: String = transaction
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='trigger' AND name=?1",
            [NAME],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restored, trigger);
    transaction.commit().unwrap();
    let committed: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='trigger' AND name=?1",
            [NAME],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(committed, trigger, "No worker resumes with weakened schema");
}
pub(crate) fn exact_sql(s: &Setup) -> Vec<String> {
    let db = writable(s);
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut out = Vec::new();
    for table in names {
        let mut q = db.prepare(&format!("SELECT * FROM {table}")).unwrap();
        let cols = q.column_count();
        let mut rows = q
            .query_map([], |r| {
                Ok((0..cols)
                    .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                    .collect::<Vec<_>>())
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        out.extend(rows.into_iter().map(|row| format!("{table}:{row:?}")));
    }
    out
}
pub(crate) fn protected_rows_v84(s: &Setup) -> Vec<String> {
    // Explicit protected-data scope, not a claim that the permitted exact
    // privacy fence leaves every derived binding/checkpoint row unchanged.
    exact_sql(s)
        .into_iter()
        .filter(|row| row.starts_with("drafts:") || row.starts_with("pr_write_"))
        .collect()
}
fn projection(s: &Setup) -> Option<crate::storage::SyncFrame> {
    Cache::open(&database(&s.fixture))
        .unwrap()
        .sync_frame(
            &Scope::github("github.invalid", Some(42)).unwrap(),
            "pr-write-projection:PERSONAL_PR_1007",
        )
        .unwrap()
}
pub(crate) fn paint(app: &mut crate::app::App) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}
pub(crate) struct StoppedV19(processes::ProcessIdentity);
impl StoppedV19 {
    pub(crate) fn final_uid(s: &Setup) -> Self {
        let until = Instant::now() + Duration::from_secs(6);
        let index = s.steps.iter().position(|s| s.delay_ms == 2000).unwrap();
        loop {
            let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
                .unwrap_or_default();
            if let Some(line) = rows.lines().nth(index) {
                let row: Value = serde_json::from_str(line).unwrap();
                assert_eq!(row["matched"], true);
                let id = processes::process_identity(row["pid"].as_i64().unwrap() as i32)
                    .unwrap()
                    .unwrap();
                assert_eq!(id.uid, unsafe { libc::geteuid() });
                assert_eq!(id.session, unsafe { libc::getsid(0) });
                assert!(!id.zombie);
                assert_eq!(unsafe { libc::kill(id.pid, libc::SIGSTOP) }, 0);
                return Self(id);
            }
            assert!(
                Instant::now() < until,
                "Actual final UID never launched: {rows}"
            );
            std::thread::yield_now();
        }
    }
}
impl Drop for StoppedV19 {
    fn drop(&mut self) {
        if let Ok(Some(now)) = processes::process_identity(self.0.pid) {
            if (now.uid, now.birth, now.session) == (self.0.uid, self.0.birth, self.0.session)
                && !now.zombie
            {
                unsafe { libc::kill(now.pid, libc::SIGCONT) };
            }
        }
    }
}
fn held_final<T>(
    s: &Setup,
    form: &crate::forms::Form,
    preview: &Arc<ConversationPreviewV17>,
    mutate: impl FnOnce() -> T,
) -> (
    crate::scoped_pr_collaboration_plan_v17::ConversationPublicationV17,
    T,
) {
    held_publication_v84(
        s,
        form,
        false,
        |api, client, fence, cancel, deadline| {
            PrPlan::execute_original_conversation_v17(
                preview,
                api,
                &database(&s.fixture),
                client,
                fence,
                cancel,
                deadline,
            )
            .unwrap()
        },
        |_, _| mutate(),
    )
}
/// Actual writer/ACK and fixed publisher, held at its real final UID. This is
/// test-only cooperation for the Base sibling, not a source/receipt factory.
pub(crate) fn held_publication_v84<T>(
    s: &Setup,
    form: &crate::forms::Form,
    narrower_clock: bool,
    execute: impl FnOnce(
            &GithubProvider,
            &crate::drafts::DraftClient,
            crate::drafts::DraftPublicationFenceV2,
            &CancellationToken,
            Instant,
        ) -> crate::scoped_pr_collaboration_plan_v17::ConversationPublicationV17
        + Send,
    observe: impl FnOnce(&crate::drafts::DraftClient, Instant) -> T,
) -> (
    crate::scoped_pr_collaboration_plan_v17::ConversationPublicationV17,
    T,
) {
    let path = database(&s.fixture);
    let mut writer = DraftWriter::new(path.clone());
    let client = writer.client();
    writer
        .save(
            form.scope(),
            form.resource(),
            ui::encoded_draft(form).unwrap(),
        )
        .unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let effective = if narrower_clock {
        Instant::now() + Duration::from_secs(2)
    } else {
        deadline
    };
    let fence = client
        .capture_publication_v2(&path, &form.scope(), &form.resource(), &cancel, deadline)
        .unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(effective, 1024 * 1024)
        .unwrap();
    std::thread::scope(|scope| {
        let api = &api;
        let client_ref = &client;
        let cancel = &cancel;
        let worker = scope.spawn(move || execute(api, client_ref, fence, cancel, deadline));
        let stopped = StoppedV19::final_uid(s);
        assert!(
            Cache::open(&path)
                .unwrap()
                .draft(&form.scope(), &form.resource())
                .unwrap()
                .is_some(),
            "No cleanup before the actual last numeric UID"
        );
        assert_eq!(
            writable(s)
                .query_row(
                    "SELECT state FROM pr_write_attempts ORDER BY attempt_id DESC LIMIT 1",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "confirmed"
        );
        let pid = stopped.0.pid;
        let value = observe(&client, effective);
        drop(stopped);
        let outcome = worker.join().unwrap();
        assert!(
            processes::process_identity(pid).unwrap().is_none(),
            "Actual exact final-UID child reaped"
        );
        (outcome, value)
    })
}
fn paid_tail() -> Vec<GhStep> {
    let mut tail = phase(PrCommand::NewComment, false, Some(true));
    tail.extend(receipts(PrCommand::NewComment));
    tail.extend(visibility());
    tail
}
pub(crate) fn unavailable_tail() -> Vec<GhStep> {
    let mut tail = phase(PrCommand::NewComment, false, Some(true));
    tail.extend(receipts(PrCommand::NewComment));
    let mut view = visibility();
    view[3].stdout = b"[]".to_vec();
    view.truncate(4);
    tail.extend(view);
    tail
}
pub(crate) fn background(tail: Vec<GhStep>) -> (Setup, Background, DraftWriter) {
    let (mut s, form, _) = prepared(PrCommand::NewComment, tail);
    let mut writer = DraftWriter::new(database(&s.fixture));
    writer
        .save(
            form.scope(),
            form.resource(),
            ui::encoded_draft(&form).unwrap(),
        )
        .unwrap();
    s.app.form = Some(form);
    s.app.overlay = Some(Overlay::Form);
    s.app.dispatch(Action::Form(FormAction::Submit));
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    let mut bg = Background::new();
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    finish(&mut s, &mut bg);
    (s, bg, writer)
}

#[test]
fn conversation_refresh_v19_held_paid_final_uid_keeps_exact_draft_until_success_and_loss_retains_it(
) {
    for uid in [42, 43] {
        let mut tail = paid_tail();
        *tail.last_mut().unwrap() = get("user", json!({"id":uid,"login":"octocat"}));
        tail.last_mut().unwrap().delay_ms = 2000;
        let (s, form, preview) = prepared(PrCommand::NewComment, tail);
        let (outcome, (body, rows)) = held_final(&s, &form, &preview, || {
            let body = Cache::open(&database(&s.fixture))
                .unwrap()
                .draft(&form.scope(), &form.resource())
                .unwrap()
                .unwrap();
            let encoded: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(encoded["values"]["body"], TEXT);
            assert!(encoded["publication"]["attempt_id"].as_i64().unwrap() > 0);
            (body, protected_rows_v84(&s))
        });
        let current = Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&form.scope(), &form.resource())
            .unwrap();
        if uid == 42 {
            assert_eq!(outcome.state, ConversationPublicationStateV17::Confirmed);
            assert!(!outcome.draft_retained());
            assert!(outcome.refreshed_page_v19.is_some());
            assert_eq!(current, None);
        } else {
            assert_eq!(
                outcome.state,
                ConversationPublicationStateV17::ConfirmedVisibilityUnavailable
            );
            assert!(outcome.draft_retained());
            assert!(outcome.refreshed_page_v19.is_none());
            assert!(outcome.return_v19.is_none());
            assert_eq!(current.as_deref(), Some(body.as_str()));
            assert_eq!(protected_rows_v84(&s), rows);
        }
        exact_protocol(&s);
    }
}

#[test]
fn conversation_refresh_v19_held_paid_uid_foreign_ack_epoch_and_inherited_expiry_never_cleanup() {
    for expired in [false, true] {
        let mut tail = paid_tail();
        tail.last_mut().unwrap().delay_ms = 2000;
        let (s, form, preview) = prepared(PrCommand::NewComment, tail);
        let (outcome, before) = held_publication_v84(
            &s,
            &form,
            expired,
            |api, client, fence, cancel, deadline| {
                PrPlan::execute_original_conversation_v17(
                    &preview,
                    api,
                    &database(&s.fixture),
                    client,
                    fence,
                    cancel,
                    deadline,
                )
                .unwrap()
            },
            |client, effective| {
                if expired {
                    assert!(effective < Instant::now() + Duration::from_secs(20));
                    while Instant::now() < effective {
                        std::thread::yield_now();
                    }
                } else {
                    let body = Cache::open(&database(&s.fixture))
                        .unwrap()
                        .draft(&form.scope(), &form.resource())
                        .unwrap()
                        .unwrap();
                    // SAME bytes through the SAME real writer retire ACK's
                    // producer epoch; body equality alone must not authorize it.
                    client
                        .save_and_wait(form.scope(), form.resource(), body)
                        .unwrap();
                }
                protected_rows_v84(&s)
            },
        );
        assert_eq!(
            outcome.state,
            ConversationPublicationStateV17::ConfirmedVisibilityUnavailable
        );
        assert!(outcome.draft_retained());
        assert!(outcome.refreshed_page_v19.is_none());
        assert_eq!(protected_rows_v84(&s), before);
        exact_protocol(&s);
    }
}
fn finish(s: &mut Setup, bg: &mut Background) {
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        bg.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Actual original worker did not finish"
        );
        std::thread::yield_now();
    }
}

#[test]
fn conversation_refresh_v19_actual_paid_ten_child_snapshot_installs_new_ticket_and_next_normal_core(
) {
    let mut tail = paid_tail();
    tail.extend(parent_phase());
    let (mut s, form, preview) = prepared(PrCommand::NewComment, tail);
    let original = ui::request(&form)
        .unwrap()
        .intent()
        .source
        .opening()
        .clone();
    let ticket = s
        .app
        .github
        .as_ref()
        .unwrap()
        .personal_pr_page_ticket_v13
        .clone()
        .unwrap();
    let mut outcome = execute(&s, &form, &preview);
    assert_eq!(outcome.state, ConversationPublicationStateV17::Confirmed);
    assert!(!outcome.draft_retained());
    assert!(outcome.return_v19.is_none());
    s.app.form = Some(form);
    s.app.overlay = Some(Overlay::Form);
    assert!(outcome
        .refreshed_page_v19
        .take()
        .unwrap()
        .install(&mut s.app));
    let g = s.app.github.as_ref().unwrap();
    assert!(!Arc::ptr_eq(
        g.personal_pr_page_ticket_v13.as_ref().unwrap(),
        &ticket
    ));
    assert_eq!(g.pr.as_ref().unwrap().details.summary.pr.body, BODY);
    assert!(g
        .pr
        .as_ref()
        .unwrap()
        .details
        .comments
        .iter()
        .any(|c| c.id == 901 && c.body == TEXT));
    assert!(!original.owns_presentation(&s.app));
    let current = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let cancel = current.cancellation().clone();
    let deadline = current.deadline();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(current.deadline(), 1024 * 1024)
        .unwrap();
    let fresh = source::open(
        current,
        PrCommand::NewComment,
        &api,
        &database(&s.fixture),
        &cancel,
        deadline,
    );
    assert!(
        fresh.is_ok(),
        "Actual next normal action still requires and completes its genuine Core"
    );
    assert!(projection(&s).is_some());
    exact_protocol(&s);
}
#[test]
fn conversation_refresh_v19_held_final_uid_raw_receipt_origin_created_and_index_drift_abort_full_transaction(
) {
    for sql in [
        "UPDATE pr_write_attempts SET confirmed_json='{}'",
        "UPDATE pr_write_attempts SET created_at=created_at+1",
        "UPDATE pr_write_conversation_origins_v17 SET origin_json=origin_json||' '",
        "UPDATE pr_write_conversation_origins_v17 SET base_key_v18='foreign'",
    ] {
        let mut tail = paid_tail();
        tail.last_mut().unwrap().delay_ms = 2000;
        let (s, form, preview) = prepared(PrCommand::NewComment, tail);
        let (outcome, after_edit) = held_final(&s, &form, &preview, || {
            corrupt_original_metadata_v19(&s, sql);
            exact_sql(&s)
        });
        assert_eq!(
            outcome.state,
            ConversationPublicationStateV17::ConfirmedVisibilityUnavailable
        );
        assert!(outcome.refreshed_page_v19.is_none());
        assert!(outcome.return_v19.is_none());
        assert!(projection(&s).is_none());
        assert_eq!(
            exact_sql(&s),
            after_edit,
            "After UID only exact pre-captured raw CAS is permitted; no stale repair/fence"
        );
        exact_protocol(&s);
    }
}
#[test]
fn conversation_refresh_v19_held_final_uid_actual_clear_and_newer_sync_payload_refuse_without_rebase(
) {
    for clear in [false, true] {
        let mut tail = paid_tail();
        tail.last_mut().unwrap().delay_ms = 2000;
        let (s, form, preview) = prepared(PrCommand::NewComment, tail);
        let (outcome, after_edit) = held_final(&s, &form, &preview, || {
            if clear {
                Cache::open(&database(&s.fixture))
                    .unwrap()
                    .clear_records(&form.scope())
                    .unwrap();
            } else {
                assert!(writable(&s).execute("UPDATE records SET complete=1-complete WHERE host='github.invalid' AND login='user-id:42'",[]).unwrap()>0);
            }
            exact_sql(&s)
        });
        assert!(outcome.refreshed_page_v19.is_none());
        assert!(outcome.return_v19.is_none());
        assert!(projection(&s).is_none());
        assert_eq!(exact_sql(&s), after_edit);
        exact_protocol(&s);
    }
}
#[test]
fn conversation_refresh_v19_actual_final_uid_loss_never_retains_old_private_workspace_return() {
    let mut tail = paid_tail();
    *tail.last_mut().unwrap() = get("user", json!({"id":43,"login":"octocat"}));
    let (s, form, preview) = prepared(PrCommand::NewComment, tail);
    let outcome = execute(&s, &form, &preview);
    assert_eq!(
        outcome.state,
        ConversationPublicationStateV17::ConfirmedVisibilityUnavailable
    );
    assert!(outcome.refreshed_page_v19.is_none());
    assert!(outcome.return_v19.is_none());
    assert!(projection(&s).is_none());
    assert!(outcome.draft_retained());
    let encoded: Value = serde_json::from_str(
        &Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&form.scope(), &form.resource())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(encoded["values"]["body"], TEXT);
    assert_eq!(
        encoded["publication"],
        serde_json::to_value(&outcome.reference).unwrap()
    );
    assert_eq!(
        writable(&s)
            .query_row("SELECT state FROM pr_write_attempts", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "confirmed"
    );
    exact_protocol(&s);
}
#[test]
fn conversation_refresh_v19_actual_cleanup_sql_refusal_retains_confirmed_return_and_exact_protected_body_without_final_uid(
) {
    // Preserve this registered legacy name, but require the final UID now:
    // a later local cleanup refusal is not a provider account observation.
    let (s, form, preview) = prepared(PrCommand::NewComment, paid_tail());
    writable(&s).execute_batch("CREATE TRIGGER v19_actual_cleanup_refused BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'conversation-draft-v17/%' BEGIN SELECT RAISE(ABORT,'actual protected cleanup refusal');END;").unwrap();
    let outcome = execute(&s, &form, &preview);
    assert_eq!(
        outcome.state,
        ConversationPublicationStateV17::ConfirmedVisibilityUnavailable
    );
    assert!(outcome.draft_retained());
    assert!(outcome.refreshed_page_v19.is_none());
    assert!(outcome.return_v19.is_some(), "Real local cleanup failure is NOT real account loss; exact native own-fence permits only the read-only Return");
    let (state,receipt,body):(String,String,String)=writable(&s).query_row("SELECT a.state,a.confirmed_json,d.body FROM pr_write_attempts a JOIN drafts d ON d.resource LIKE 'conversation-draft-v17/%' WHERE a.attempt_id=?1",[outcome.reference.attempt_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(state, "confirmed");
    assert_eq!(
        serde_json::from_str::<Value>(&receipt).unwrap()["resource_id"],
        "901"
    );
    let encoded: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(encoded["values"]["body"], TEXT);
    assert_eq!(
        encoded["publication"],
        serde_json::to_value(&outcome.reference).unwrap()
    );
    assert!(projection(&s).is_none());
    assert_eq!(writable(&s).query_row("SELECT count(*) FROM sync_mutation_fences WHERE resource='PERSONAL_PR_1007' AND kind='conversation_comment'",[],|r|r.get::<_,usize>(0)).unwrap(),1);
    exact_protocol(&s);
}
#[test]
fn conversation_refresh_v19_actual_projection_sql_fault_rolls_back_rows_then_only_exact_privacy_fence(
) {
    let (s, form, preview) = prepared(PrCommand::NewComment, paid_tail());
    writable(&s).execute_batch("CREATE TRIGGER v19_actual_projection_fault BEFORE INSERT ON records WHEN NEW.kind='collaboration-document-v1' BEGIN SELECT RAISE(ABORT,'actual projection transaction refusal');END;").unwrap();
    let outcome = execute(&s, &form, &preview);
    assert_eq!(
        outcome.state,
        ConversationPublicationStateV17::ConfirmedVisibilityUnavailable
    );
    assert!(outcome.refreshed_page_v19.is_none());
    assert!(outcome.return_v19.is_some());
    assert!(projection(&s).is_none());
    assert_eq!(writable(&s).query_row("SELECT count(*) FROM records WHERE kind IN('collaboration-document-v1','collaboration-state-v1')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
    assert_eq!(writable(&s).query_row("SELECT count(*) FROM sync_mutation_fences WHERE resource='PERSONAL_PR_1007' AND kind='conversation_comment'",[],|r|r.get::<_,usize>(0)).unwrap(),1);
    exact_protocol(&s);
}
#[test]
fn conversation_refresh_v19_actual_background_return_doc_capture_and_pure_back_have_zero_io() {
    let (mut s, mut bg, _writer) = background(unavailable_tail());
    assert!(s.app.github.is_none());
    assert!(s.app.scoped_pr_collaboration_return_v19.is_some());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    let text = paint(&mut s.app);
    assert!(text.contains("DURABLY CONFIRMED"));
    assert!(!text.contains(BODY));
    assert!(!text.contains(TEXT));
    assert!(ui::return_ready_v19(s.app.form.as_ref().unwrap()));
    let sql = exact_sql(&s);
    let commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let request =
        crate::scoped_pr_collaboration_refresh_v19::return_v19::capture_request(&s.app).unwrap();
    assert!(request.owns(&s.app));
    assert!(channel::generation(&bg).is_none());
    assert!(crate::scoped_pr_collaboration_refresh_v19::return_v19::back(&mut s.app));
    assert!(s.app.scoped_pr_collaboration_return_v19.is_none());
    let g = s.app.github.as_ref().unwrap();
    assert!(g.pr.is_none());
    assert!(g.personal_pr_page_ticket_v13.is_none());
    assert!(g.scoped_pr_navigation.is_none());
    assert!(!bg.poll(&mut s.app));
    assert_eq!(exact_sql(&s), sql);
    assert_eq!(
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        commands
    );
    exact_protocol(&s);
}
#[test]
fn conversation_refresh_v19_actual_new_thirteen_child_read_has_same_client_and_no_additional_write()
{
    let mut tail = unavailable_tail();
    tail.extend(
        crate::personal_pr_entry_v13::tests::collaboration_v17::refresh_v19::refresh_phase(),
    );
    let (mut s, mut bg, writer) = background(tail);
    paint(&mut s.app);
    s.app
        .dispatch(Action::Form(FormAction::RefreshConversationV19));
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    finish(&mut s, &mut bg);
    assert!(s.app.github.as_ref().unwrap().pr.is_some());
    assert!(s.app.scoped_pr_collaboration_return_v19.is_none());
    assert!(projection(&s).is_some());
    assert!(s.app.form.is_none());
    exact_protocol(&s);
}
#[test]
fn conversation_refresh_v19_failed_original_personal_reservation_disables_retry_then_safe_back_no_replay(
) {
    let (mut s, mut bg, writer) = background(unavailable_tail());
    paint(&mut s.app);
    let row:(String,u64)=writable(&s).query_row("SELECT source,generation FROM sync_reservations WHERE source LIKE 'personal-pr-source-v13/%'",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(
        writable(&s)
            .execute(
                "UPDATE sync_reservations SET generation=generation+1 WHERE source=?1",
                [&row.0]
            )
            .unwrap(),
        1
    );
    let before = exact_sql(&s);
    let commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    s.app
        .dispatch(Action::Form(FormAction::RefreshConversationV19));
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    finish(&mut s, &mut bg);
    let text = paint(&mut s.app);
    assert!(text.contains("Back and reopen"));
    assert!(!ui::return_ready_v19(s.app.form.as_ref().unwrap()));
    s.app
        .dispatch(Action::Form(FormAction::RefreshConversationV19));
    native::prepare_requested(&mut s.app);
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert_eq!(exact_sql(&s), before);
    assert_eq!(
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        commands
    );
    assert!(crate::scoped_pr_collaboration_refresh_v19::return_v19::back(&mut s.app));
    assert!(s.app.github.as_ref().unwrap().pr.is_none());
    assert_eq!(exact_sql(&s), before);
    exact_protocol(&s);
}
