//! Genuine Personal Core, actual committed reservation and same publisher.
//! Direct workers prove SQL/phase contracts, not prequeue or binary coverage.
use super::*;
use crate::{
    app::Action,
    drafts::DraftWriter,
    forms::FormAction,
    personal_pr_entry_v13::tests::{self as personal, Setup},
    runtime::Background,
    storage::conversation_refresh_v19::observe_next_actual_reservation_v20,
    test_support::GhStep,
};
use personal::collaboration_v17::{
    protocol::publication::{self as publication, refresh_v19 as fixture},
    refresh_v19::refresh_phase,
};
use serde_json::{json, Value};
use std::{fs, time::Duration};

fn ready(phases: Vec<GhStep>) -> (Setup, Background, DraftWriter) {
    let mut tail = fixture::unavailable_tail();
    tail.extend(phases);
    let (mut s, bg, writer) = fixture::background(tail);
    assert!(fixture::paint(&mut s.app).contains("DURABLY CONFIRMED"));
    assert!(ui::return_ready_v19(s.app.form.as_ref().unwrap()));
    (s, bg, writer)
}
fn db(s: &Setup) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        personal::database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap()
}
fn generation(s: &Setup) -> (String, u64, bool) {
    db(s).query_row("SELECT source,generation,active FROM sync_reservations WHERE source LIKE 'personal-pr-source-v13/%'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
}
fn protected(s: &Setup) -> Vec<String> {
    fixture::exact_sql(s)
        .into_iter()
        .filter(|r| {
            r.starts_with("pr_write_")
                || r.starts_with("drafts:")
                || r.starts_with("sync_mutation_fences:")
                || r.contains("user-id:43")
        })
        .collect()
}
fn calls(s: &Setup) -> Vec<Value> {
    fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn run(
    s: &Setup,
    request: &ConversationRefreshRequestV19,
) -> GithubResult<AdmittedConversationPageV19> {
    run_bounded(
        s,
        request,
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(30),
        1024 * 1024,
    )
}
fn run_bounded(
    s: &Setup,
    request: &ConversationRefreshRequestV19,
    cancel: &CancellationToken,
    deadline: Instant,
    cap: usize,
) -> GithubResult<AdmittedConversationPageV19> {
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, cap)
        .unwrap();
    request.execute(&api, &personal::database(&s.fixture), cancel, deadline)
}
fn fail_at(index: usize) -> Vec<GhStep> {
    let mut phase = refresh_phase();
    phase[index].status = 1;
    phase[index].stdout.clear();
    phase[index].stderr = b"ACTUAL_V20_EXPLICIT_READ_TRANSPORT_FAILURE".to_vec();
    phase.truncate(index + 1);
    phase
}
fn finish(s: &mut Setup, bg: &mut Background) {
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        bg.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Actual same-purpose read did not settle"
        );
        std::thread::yield_now();
    }
}
fn enqueue(s: &mut Setup, bg: &mut Background, writer: &DraftWriter) {
    fixture::paint(&mut s.app);
    s.app
        .dispatch(Action::Form(FormAction::RefreshConversationV19));
    crate::scoped_pr_collaboration_native_v17::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        personal::database(&s.fixture),
        writer.client()
    ));
}

#[test]
fn conversation_refresh_retry_v20_two_actual_committed_failures_then_full_thirteen_child_page() {
    let mut phases = fail_at(4);
    phases.extend(fail_at(9));
    phases.extend(refresh_phase());
    let (mut s, _bg, _writer) = ready(phases);
    let kept = protected(&s);
    let initial = generation(&s).1;
    let request = capture_request(&s.app).unwrap();
    let start = calls(&s).len();
    assert!(run(&s, &request).is_err());
    assert_eq!(calls(&s).len(), start + 5);
    assert_eq!(generation(&s).1, initial + 1);
    assert!(request.descriptor.read_ready_v20());
    assert_eq!(protected(&s), kept);
    assert!(run(&s, &request).is_err());
    assert_eq!(calls(&s).len(), start + 15);
    assert_eq!(generation(&s).1, initial + 2);
    assert!(request.descriptor.read_ready_v20());
    assert_eq!(protected(&s), kept);
    let page = run(&s, &request).unwrap();
    assert_eq!(calls(&s).len(), start + 28);
    assert!(request.descriptor.consumed_v19());
    assert!(!request.descriptor.read_ready_v20());
    assert!(page.install_return_v19(&mut s.app, &request));
    let g = s.app.github.as_ref().unwrap();
    assert_eq!(
        g.pr.as_ref().unwrap().details.summary.pr.body,
        personal::BODY
    );
    assert_eq!(
        g.pr.as_ref().unwrap().details.comments[0].author.login,
        "other-author"
    );
    assert!(g.personal_pr_page_ticket_v13.is_some());
    assert!(s.app.form.is_none());
    assert!(s.app.scoped_pr_collaboration_return_v19.is_none());
    assert_eq!(protected(&s), kept);
    publication::exact_protocol(&s);
}

#[test]
fn conversation_refresh_retry_v20_before_reservation_and_actual_transaction_rollback_keep_predecessor(
) {
    for rollback in [false, true] {
        let mut first = if rollback {
            refresh_phase()[..4].to_vec()
        } else {
            fail_at(2)
        };
        first.extend(refresh_phase());
        let (mut s, _bg, _writer) = ready(first);
        let before = fixture::exact_sql(&s);
        let original = generation(&s).1;
        if rollback {
            db(&s).execute_batch("CREATE TRIGGER v20_refuse_reservation BEFORE UPDATE ON sync_reservations WHEN OLD.source LIKE 'personal-pr-source-v13/%' BEGIN SELECT RAISE(ABORT,'actual reservation rollback');END;").unwrap();
        }
        let request = capture_request(&s.app).unwrap();
        assert!(run(&s, &request).is_err());
        assert_eq!(fixture::exact_sql(&s), before);
        assert_eq!(generation(&s).1, original);
        assert!(request.descriptor.read_ready_v20());
        if rollback {
            db(&s)
                .execute_batch("DROP TRIGGER v20_refuse_reservation")
                .unwrap();
        }
        assert!(run(&s, &request)
            .unwrap()
            .install_return_v19(&mut s.app, &request));
        publication::exact_protocol(&s);
    }
}

#[test]
fn conversation_refresh_retry_v20_actual_postcommit_cancel_and_expiry_install_witness_before_live_refusal(
) {
    for expired in [false, true] {
        let mut first = refresh_phase()[..4].to_vec();
        first.extend(refresh_phase());
        let (mut s, _bg, _writer) = ready(first);
        let request = capture_request(&s.app).unwrap();
        let cancel = CancellationToken::default();
        let signal = cancel.clone();
        let descriptor = Arc::clone(&request.descriptor);
        let old = generation(&s).1;
        let database = personal::database(&s.fixture);
        let deadline = Instant::now()
            + if expired {
                Duration::from_secs(2)
            } else {
                Duration::from_secs(30)
            };
        observe_next_actual_reservation_v20(move || {
            assert!(
                !descriptor.read_ready_v20(),
                "Actual worker has not settled"
            );
            assert!(descriptor
                .personal_read_v20
                .lock()
                .is_ok_and(|s| matches!(*s, PersonalReadStateV20::InFlight(_))));
            let connection = rusqlite::Connection::open(database).unwrap();
            assert_eq!(connection.query_row("SELECT generation FROM sync_reservations WHERE source LIKE 'personal-pr-source-v13/%'",[],|r|r.get::<_,u64>(0)).unwrap(),old+1);
            if expired {
                while Instant::now() < deadline {
                    std::thread::yield_now();
                }
            } else {
                signal.cancel();
            }
        });
        assert!(run_bounded(&s, &request, &cancel, deadline, 1024 * 1024).is_err());
        assert_eq!(generation(&s).1, old + 1);
        assert!(request.descriptor.read_ready_v20());
        assert!(run(&s, &request)
            .unwrap()
            .install_return_v19(&mut s.app, &request));
        publication::exact_protocol(&s);
    }
}

#[test]
fn conversation_refresh_retry_v20_actual_final_uid_transport_can_retry_but_different_uid_erases_private_return(
) {
    for lost in [false, true] {
        let mut first = if lost {
            let mut p = refresh_phase();
            p[12] = personal::get("user", json!({"id":43,"login":"octocat"}));
            p
        } else {
            fail_at(12)
        };
        if !lost {
            first.extend(refresh_phase());
        }
        let (mut s, mut bg, writer) = ready(first);
        let kept = protected(&s);
        enqueue(&mut s, &mut bg, &writer);
        finish(&mut s, &mut bg);
        if lost {
            assert!(s.app.github.is_none());
            assert!(s.app.form.is_none());
            assert!(s.app.scoped_pr_collaboration_return_v19.is_none());
            let text = fixture::paint(&mut s.app);
            assert!(!text.contains(personal::BODY));
            assert!(!text.contains("Actual fresh discussion"));
        } else {
            fixture::paint(&mut s.app);
            assert!(ui::return_ready_v19(s.app.form.as_ref().unwrap()));
            enqueue(&mut s, &mut bg, &writer);
            finish(&mut s, &mut bg);
            assert!(s.app.github.as_ref().unwrap().pr.is_some());
        }
        assert_eq!(protected(&s), kept);
        publication::exact_protocol(&s);
    }
}

#[test]
fn conversation_refresh_retry_v20_clear_newer_row_payload_terminal_and_once_fence_never_rebase() {
    for mode in 0..5 {
        let (s, _bg, _writer) = ready(fail_at(4));
        let request = capture_request(&s.app).unwrap();
        assert!(run(&s, &request).is_err());
        match mode {
            0=>crate::storage::Cache::open(&personal::database(&s.fixture)).unwrap().clear_records(request.original().scope()).unwrap(),
            1=>assert_eq!(db(&s).execute("UPDATE sync_reservations SET generation=generation+1 WHERE source LIKE 'personal-pr-source-v13/%'",[]).unwrap(),1),
            2=>assert!(db(&s).execute("UPDATE records SET complete=1-complete WHERE host='github.invalid' AND login='user-id:42'",[]).unwrap()>0),
            3=>assert_eq!(db(&s).execute("UPDATE pr_write_attempts SET created_at=created_at+1",[]).unwrap(),1),
            _=>assert_eq!(db(&s).execute("UPDATE sync_mutation_fences SET mutation_ordinal=mutation_ordinal+1 WHERE resource='PERSONAL_PR_1007' AND kind='conversation_comment'",[]).unwrap(),1),
        }
        let before = fixture::exact_sql(&s);
        let commands = calls(&s);
        assert!(run(&s, &request).is_err());
        assert!(request.descriptor.consumed_v19());
        assert_eq!(calls(&s), commands);
        assert_eq!(fixture::exact_sql(&s), before);
        publication::exact_protocol(&s);
    }
}

#[test]
fn conversation_refresh_retry_v20_foreign_actual_runner_cannot_start_core_or_replace_last_witness()
{
    let (s, _bg, _writer) = ready(Vec::new());
    let foreign = personal::setup(Vec::new());
    let request = capture_request(&s.app).unwrap();
    let before = fixture::exact_sql(&s);
    let original = calls(&s);
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = foreign
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    assert!(request
        .execute(
            &api,
            &personal::database(&s.fixture),
            &CancellationToken::default(),
            deadline
        )
        .is_err());
    assert_eq!(fixture::exact_sql(&s), before);
    assert_eq!(calls(&s), original);
    assert!(request.descriptor.consumed_v19());
    publication::exact_protocol(&s);
    personal::exact(&foreign, foreign.steps.len());
}

#[test]
fn conversation_refresh_retry_v20_no_overlapping_phase_or_copied_token_and_unavailable_stays_closed(
) {
    let (s, _bg, _writer) = ready(Vec::new());
    let request = capture_request(&s.app).unwrap();
    let own = request.descriptor.own.get().unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let attempt = own
        .begin_personal_read_attempt_v20(None, &s.api, &cancel, deadline)
        .unwrap();
    assert!(attempt
        .matches_phase(&CancellationToken::default(), deadline)
        .is_err());
    assert!(attempt
        .matches_phase(&cancel, deadline + Duration::from_millis(1))
        .is_err());
    let ticket = Arc::new(());
    *request.descriptor.personal_read_v20.lock().unwrap() =
        PersonalReadStateV20::InFlight(ticket.clone());
    let before = fixture::exact_sql(&s);
    let commands = calls(&s);
    assert!(run(&s, &request).is_err());
    assert!(capture_request(&s.app).is_err());
    drop(PersonalReadSettlementV20 {
        descriptor: request.descriptor.clone(),
        ticket,
        previous: None,
        attempt: Some(attempt),
        unavailable: true,
    });
    assert!(request.descriptor.consumed_v19());
    assert!(!request.descriptor.read_ready_v20());
    assert_eq!(fixture::exact_sql(&s), before);
    assert_eq!(calls(&s), commands);
    publication::exact_protocol(&s);
}

#[test]
fn conversation_refresh_retry_v20_actual_panic_after_commit_settles_only_known_witness_then_next_full_read(
) {
    let mut first = refresh_phase()[..4].to_vec();
    first.extend(refresh_phase());
    let (mut s, _bg, _writer) = ready(first);
    let request = capture_request(&s.app).unwrap();
    let old = generation(&s).1;
    observe_next_actual_reservation_v20(|| {
        panic!("ACTUAL_V20_PANIC_AFTER_GENUINE_RESERVATION_COMMIT")
    });
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&s, &request))).is_err());
    assert_eq!(generation(&s).1, old + 1);
    assert!(request.descriptor.read_ready_v20());
    assert!(run(&s, &request)
        .unwrap()
        .install_return_v19(&mut s.app, &request));
    publication::exact_protocol(&s);
}

#[test]
fn conversation_refresh_retry_v20_new_form_and_normal_view_do_not_gain_old_return_or_effects() {
    let (mut s, _bg, _writer) = ready(fail_at(4));
    let request = capture_request(&s.app).unwrap();
    assert!(run(&s, &request).is_err());
    let (mut newer, _other, _drafts) = ready(Vec::new());
    let newer_ticket = Arc::clone(newer.app.form.as_ref().unwrap().presentation_v4().0);
    let old = std::mem::replace(&mut s.app, newer.app);
    newer.app = old;
    s.app.dispatch(Action::View(View::Repository));
    let status = s.app.status.clone();
    let loading = s.app.loading;
    assert!(!request.owns(&s.app));
    let before = fixture::exact_sql(&s);
    let commands = calls(&s);
    retire_orphan_v19(&mut s.app, &request);
    assert!(Arc::ptr_eq(
        s.app.form.as_ref().unwrap().presentation_v4().0,
        &newer_ticket
    ));
    assert_eq!(s.app.status, status);
    assert_eq!(s.app.loading, loading);
    assert_eq!(fixture::exact_sql(&s), before);
    assert_eq!(calls(&s), commands);
    publication::exact_protocol(&s);
    publication::exact_protocol(&newer);
}

#[test]
fn conversation_refresh_retry_v20_actual_projection_insert_rollback_keeps_only_last_real_reservation(
) {
    let mut phases = refresh_phase();
    phases.extend(refresh_phase());
    let (mut s, _bg, _writer) = ready(phases);
    let request = capture_request(&s.app).unwrap();
    let kept = protected(&s);
    let old = generation(&s).1;
    db(&s).execute_batch("CREATE TRIGGER v20_fail_projection BEFORE INSERT ON records WHEN NEW.kind='collaboration-document-v1' BEGIN SELECT RAISE(ABORT,'actual final projection rollback');END;").unwrap();
    assert!(run(&s, &request).is_err());
    assert_eq!(protected(&s), kept);
    assert_eq!(generation(&s).1, old + 1);
    assert_eq!(db(&s).query_row("SELECT count(*) FROM records WHERE kind IN('collaboration-document-v1','collaboration-state-v1')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
    assert!(request.descriptor.read_ready_v20());
    db(&s)
        .execute_batch("DROP TRIGGER v20_fail_projection")
        .unwrap();
    assert!(run(&s, &request)
        .unwrap()
        .install_return_v19(&mut s.app, &request));
    assert_eq!(protected(&s), kept);
    publication::exact_protocol(&s);
}

#[test]
fn conversation_refresh_retry_v20_useful_geometry_no_color_and_settling_disable_draw_without_io() {
    let (mut s, _bg, _writer) = ready(Vec::new());
    let request = capture_request(&s.app).unwrap();
    let before = fixture::exact_sql(&s);
    let commands = calls(&s);
    for no_color in [false, true] {
        for (width, height) in [(80, 24), (120, 40)] {
            s.app.no_color = no_color;
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| crate::ui::draw(f, &mut s.app)).unwrap();
            assert!(ui::return_ready_v19(s.app.form.as_ref().unwrap()));
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("[r Refresh PR]"));
            assert!(text.contains("[Esc Back to Inbox]"));
            assert!(!text.contains(personal::BODY));
        }
    }
    let focused = s.app.form.as_ref().unwrap().focus;
    {
        use crossterm::event::{KeyCode, KeyModifiers};
        let form = s.app.form.as_mut().unwrap();
        assert_eq!(form.key(KeyCode::Tab, KeyModifiers::NONE), None);
        assert_ne!(form.focus, focused);
        assert_eq!(form.key(KeyCode::BackTab, KeyModifiers::SHIFT), None);
        assert_eq!(form.focus, focused);
        assert_eq!(
            form.key(KeyCode::Enter, KeyModifiers::NONE),
            Some(FormAction::RefreshConversationV19)
        );
    }
    fixture::paint(&mut s.app);
    let click = s
        .app
        .hits
        .iter()
        .find(|hit| hit.action == Action::Form(FormAction::RefreshConversationV19))
        .unwrap()
        .clone();
    assert!(click.rect.width > 0 && click.rect.height > 0);
    s.app.dispatch(click.action);
    assert!(s.app.form.as_ref().unwrap().preview_requested);
    // No worker preparation is invoked by this paint/input-only contract.
    s.app.form.as_mut().unwrap().preview_requested = false;
    *request.descriptor.personal_read_v20.lock().unwrap() =
        PersonalReadStateV20::InFlight(Arc::new(()));
    fixture::paint(&mut s.app);
    assert!(!ui::return_ready_v19(s.app.form.as_ref().unwrap()));
    assert!(forward_refresh(&mut s.app));
    assert!(!s.app.form.as_ref().unwrap().preview_requested);
    assert_eq!(fixture::exact_sql(&s), before);
    assert_eq!(calls(&s), commands);
    publication::exact_protocol(&s);
}

#[test]
fn conversation_refresh_retry_v20_expired_new_phase_and_original_64k_child_cap_do_not_renew_or_widen(
) {
    let mut phase = refresh_phase();
    let mut raw: Value = serde_json::from_slice(&phase[3].stdout).unwrap();
    raw["body"] = json!("x".repeat(70 * 1024));
    phase[3].stdout = serde_json::to_vec(&raw).unwrap();
    phase.truncate(4);
    let (s, _bg, _writer) = ready(phase);
    let request = capture_request(&s.app).unwrap();
    let before = fixture::exact_sql(&s);
    let commands = calls(&s);
    assert!(run_bounded(
        &s,
        &request,
        &CancellationToken::default(),
        Instant::now() - Duration::from_millis(1),
        64 * 1024
    )
    .is_err());
    assert_eq!(calls(&s), commands);
    assert_eq!(fixture::exact_sql(&s), before);
    assert!(request.descriptor.read_ready_v20());
    assert!(run_bounded(
        &s,
        &request,
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(30),
        64 * 1024
    )
    .is_err());
    assert_eq!(fixture::exact_sql(&s), before);
    assert!(request.descriptor.read_ready_v20());
    publication::exact_protocol(&s);
}

#[test]
fn conversation_refresh_retry_v20_same_tuple_foreign_actual_core_arc_cannot_close_current_boundary()
{
    let mut phase = refresh_phase()[..4].to_vec();
    phase.extend(refresh_phase()[..3].iter().cloned());
    let (s, _bg, _writer) = ready(phase);
    let request = capture_request(&s.app).unwrap();
    let own = request.descriptor.own.get().unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let attempt = own
        .begin_personal_read_attempt_v20(None, &api, &cancel, deadline)
        .unwrap();
    let limits = crate::providers::gh::RepositoryCatalogueAccessLimits {
        max_repository_reads: 2,
        ..Default::default()
    };
    let parent_intent = request.original().personal().unwrap().parent();
    let mut first = api
        .begin_personal_pr_access_v13(parent_intent, &limits, &cancel)
        .unwrap();
    let measured = api
        .clone()
        .with_measured_read_window(
            first
                .reserve_measured_external_reads(1, 128 * 1024, 128 * 1024, &cancel)
                .unwrap(),
        )
        .unwrap();
    let parent = Arc::new(
        measured
            .pr_original_detail_parent_v17(first.observation(), parent_intent, &cancel)
            .unwrap(),
    );
    let mut cache = crate::storage::Cache::open(&personal::database(&s.fixture)).unwrap();
    let prepared = cache
        .prepare_personal_conversation_read_boundary_v20(
            &attempt,
            first.original(),
            parent.clone(),
            crate::providers::gh::ThreadPhaseIssuerV16::capture(&api).unwrap(),
            &cancel,
            deadline,
        )
        .unwrap();
    let other = api
        .begin_personal_pr_access_v13(parent_intent, &limits, &cancel)
        .unwrap();
    assert!(!Arc::ptr_eq(first.original(), other.original()));
    let before = fixture::exact_sql(&s);
    assert!(prepared
        .verify_for_close(other.original(), &parent)
        .is_err());
    assert!(prepared.verify_for_close(first.original(), &parent).is_ok());
    assert_eq!(fixture::exact_sql(&s), before);
    publication::exact_protocol(&s);
}

#[test]
fn conversation_refresh_retry_v20_actual_replaced_database_never_reconstructs_committed_row_lineage(
) {
    use std::os::unix::fs::MetadataExt;
    let (s, bg, writer) = ready(fail_at(4));
    let request = capture_request(&s.app).unwrap();
    assert!(run(&s, &request).is_err());
    // End the actual clients before replacing ONLY this disposable fixture DB.
    // Its original file is preserved, not erased or overwritten.
    drop(writer);
    drop(bg);
    let path = personal::database(&s.fixture);
    let old = s.fixture.root.join("v20-original-preserved.sqlite3");
    let replacement = s.fixture.root.join("v20-replacement.sqlite3");
    let fresh = crate::storage::Cache::open(&replacement).unwrap();
    for uid in [42, 43] {
        fresh
            .put_draft(
                &crate::storage::Scope::github("github.invalid", Some(uid)).unwrap(),
                "kept",
                &format!("PRIVATE_{uid}"),
            )
            .unwrap();
    }
    drop(fresh);
    let inode = fs::metadata(&path).unwrap().ino();
    fs::rename(&path, &old).unwrap();
    fs::rename(&replacement, &path).unwrap();
    assert_ne!(fs::metadata(&path).unwrap().ino(), inode);
    let before = fixture::exact_sql(&s);
    let commands = calls(&s);
    assert!(run(&s, &request).is_err());
    assert!(request.descriptor.consumed_v19());
    assert_eq!(fixture::exact_sql(&s), before);
    assert_eq!(calls(&s), commands);
    let preserved =
        rusqlite::Connection::open_with_flags(&old, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    assert_eq!(
        preserved
            .query_row("SELECT state FROM pr_write_attempts", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "confirmed"
    );
    publication::exact_protocol(&s);
}
