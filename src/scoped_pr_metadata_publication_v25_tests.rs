//! Actual same Core/SQLite/PrPlan publication and explicit native Form dispatch.
use super::*;
use crate::{
    drafts::DraftWriter,
    forms::{Form, FormAction},
    pr_actions::PrPlan,
    pr_write_journal::{PrWriteJournal, PrWriteState, PrWriteTarget},
    scoped_pr_collaboration_plan_v17::ConversationPublicationStateV17 as State,
    scoped_pr_collaboration_ui_v17::{self as ui, metadata_v25 as metadata},
};
#[path = "scoped_pr_metadata_manual_refresh_v45_tests.rs"]
mod manual_refresh_v45;
#[path = "scoped_pr_metadata_nullable_lineage_v57_tests.rs"]
mod nullable_lineage_v57;
fn full_raw(field: Field, requested: bool) -> Value {
    let mut raw = raw(json!(CURRENT));
    raw["additions"] = json!(1);
    raw["deletions"] = json!(1);
    raw["created_at"] = json!("2026-10-16T01:00:00Z");
    raw["updated_at"] = json!("2026-10-16T02:00:00Z");
    raw["mergeable"] = Value::Null;
    raw["mergeable_state"] = json!("unknown");
    if requested {
        raw[if field == Field::Title {
            "title"
        } else {
            "body"
        }] = json!(REQUESTED);
    }
    raw
}
fn receipts(field: Field) -> Vec<GhStep> {
    let endpoint = format!("repos/{REPO}/pulls/7");
    let mut patch = get(&endpoint, full_raw(field, true));
    *patch.args.iter_mut().find(|a| a.as_str() == "GET").unwrap() = "PATCH".into();
    patch.args.extend(["--input".into(), "-".into()]);
    patch.stdin = serde_json::to_vec(&if field == Field::Title {
        json!({"title":REQUESTED})
    } else {
        json!({"body":REQUESTED})
    })
    .unwrap();
    vec![patch, get(&endpoint, full_raw(field, true))]
}
fn visibility(field: Field) -> Vec<GhStep> {
    let mut calls = vec![
        get("user", json!({"id":42,"login":"octocat"})),
        get(&format!("repos/{REPO}"), repository()),
        get(&format!("repos/{REPO}/pulls/7"), full_raw(field, true)),
    ];
    for (endpoint, rows) in [
        ("issues/7/comments", json!([])),
        ("pulls/7/commits", json!([])),
        (
            "pulls/7/files",
            json!([{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]),
        ),
        ("pulls/7/reviews", json!([])),
        ("issues/7/timeline", json!([])),
    ] {
        let mut child = get(
            &format!("repos/{REPO}/{endpoint}?per_page=100"),
            json!([rows]),
        );
        child.args.extend(["--paginate".into(), "--slurp".into()]);
        calls.push(child);
    }
    calls.push(get(&format!("repos/{REPO}/pulls/7"), full_raw(field, true)));
    calls.push(get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(calls.len(), 10);
    calls
}
fn paid(field: Field) -> Vec<GhStep> {
    let mut calls = phase(full_raw(field, false), capability(json!(true)));
    calls.extend(receipts(field));
    calls.extend(visibility(field));
    assert_eq!(calls.len(), 21);
    calls
}
fn prepared(field: Field, tail: Vec<GhStep>) -> (Setup, Form, Arc<metadata::MetadataPreviewV25>) {
    prepared_display_v44(field, tail, true)
}
fn prepared_display_v44(
    field: Field,
    tail: Vec<GhStep>,
    paint: bool,
) -> (Setup, Form, Arc<metadata::MetadataPreviewV25>) {
    let mut calls = phase(full_raw(field, false), capability(json!(true)));
    calls.extend(phase(full_raw(field, false), capability(json!(true))));
    calls.extend(tail);
    let mut s = setup(calls);
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
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
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let preview = metadata::MetadataPreviewV25::prepare(
        &request,
        &api,
        &database(&s.fixture),
        &cancel,
        deadline,
    )
    .unwrap();
    assert!(metadata::accept_preview(
        &mut form,
        &request,
        preview.clone()
    ));
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(crate::app::Overlay::Form);
    if paint {
        paint_form_v44(&mut s.app, 120, 40);
    }
    assert_eq!(
        metadata::request(s.app.form.as_ref().unwrap())
            .unwrap()
            .matches_confirm(s.app.form.as_ref().unwrap()),
        paint
    );
    let form = s.app.form.take().unwrap();
    s.app.overlay = None;
    (s, form, preview)
}
fn execute(
    s: &Setup,
    form: &Form,
    preview: &Arc<metadata::MetadataPreviewV25>,
) -> crate::providers::gh::GithubResult<
    crate::scoped_pr_collaboration_plan_v17::ConversationPublicationV17,
> {
    execute_with_issuer_v51(s, form, preview, None)
}
fn execute_with_issuer_v51(
    s: &Setup,
    form: &Form,
    preview: &Arc<metadata::MetadataPreviewV25>,
    issuer: Option<Instant>,
) -> crate::providers::gh::GithubResult<
    crate::scoped_pr_collaboration_plan_v17::ConversationPublicationV17,
> {
    let mut writer = DraftWriter::new(database(&s.fixture));
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
    let fence = client
        .capture_publication_v2(
            &database(&s.fixture),
            &form.scope(),
            &form.resource(),
            &cancel,
            deadline,
        )
        .unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(issuer.unwrap_or(deadline), 1024 * 1024)
        .unwrap();
    PrPlan::execute_original_metadata_text_v25(
        preview,
        &api,
        &database(&s.fixture),
        &client,
        fence,
        &cancel,
        deadline,
    )
}
pub(super) fn exact(s: &Setup, count: usize) {
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    for (i, (row, step)) in rows.iter().zip(&s.steps).enumerate() {
        assert_eq!(row["step"], i);
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (path, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(path)).unwrap(), expected);
    }
    for uid in [42, 43] {
        assert_eq!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .draft(&Scope::github("github.invalid", Some(uid)).unwrap(), "kept")
                .unwrap()
                .as_deref(),
            Some(format!("PRIVATE_{uid}").as_str())
        );
    }
}
fn target(_s: &Setup, form: &Form) -> PrWriteTarget {
    let state = metadata::state(form).unwrap();
    PrWriteTarget::new(
        state.intent.source.identity(),
        state.intent.source.parent().summary(),
    )
    .unwrap()
}
fn no_resend(s: &Setup, form: &Form, preview: &Arc<metadata::MetadataPreviewV25>) {
    // No new normal save: the exact protected original envelope must remain.
    let writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let fence = client
        .capture_publication_v2(
            &database(&s.fixture),
            &form.scope(),
            &form.resource(),
            &cancel,
            deadline,
        )
        .unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    assert!(PrPlan::execute_original_metadata_text_v25(
        preview,
        &api,
        &database(&s.fixture),
        &client,
        fence,
        &cancel,
        deadline
    )
    .is_err());
}

#[test]
fn pr_metadata_original_v25_actual_title_and_description_single_field_patch_exact21_and_atomic_fresh_page(
) {
    for field in [Field::Title, Field::Description] {
        let (s, form, preview) = prepared(field, paid(field));
        let outcome = execute(&s, &form, &preview).unwrap();
        assert_eq!(outcome.state, State::Confirmed);
        assert!(!outcome.draft_retained());
        assert!(outcome.refreshed_page_v19.is_some());
        let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let target = target(&s, &form);
        let marker = journal
            .original_publication(&target, &outcome.reference)
            .unwrap()
            .unwrap();
        assert_eq!(marker.state, PrWriteState::Confirmed);
        assert_eq!(marker.command, field.command());
        assert!(journal.metadata_origin_v25(&target, &marker).is_ok());
        assert!(journal.blocking(&target).unwrap().is_none());
        assert!(Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(target.scope(), &form.resource())
            .unwrap()
            .is_none());
        let record = Cache::open(&database(&s.fixture))
            .unwrap()
            .get(target.scope(), "pr-details", target.node_id())
            .unwrap()
            .unwrap();
        assert_eq!(
            record.payload["summary"]["pr"][if field == Field::Title {
                "title"
            } else {
                "body"
            }],
            REQUESTED
        );
        assert_eq!(s.steps.len() - opening().len(), 9 + 9 + 21);
        exact(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_original_v25_real_origin_insert_failure_rolls_back_attempt_and_zero_patch() {
    let field = Field::Description;
    let tail = phase(full_raw(field, false), capability(json!(true)));
    let (s, form, preview) = prepared(field, tail);
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER fail_metadata_origin_v25 BEFORE INSERT ON pr_write_metadata_origins_v25 BEGIN SELECT RAISE(ABORT,'actual origin insertion refuses'); END;").unwrap();
    assert!(execute(&s, &form, &preview).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_metadata_origins_v25",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    exact(&s, opening().len() + 18 + 5);
}
#[test]
fn pr_metadata_original_v25_wrong_typed_patch_or_get_receipt_stays_uncertain_never_retries() {
    for child in [0, 1] {
        let field = Field::Title;
        let mut tail = phase(full_raw(field, false), capability(json!(true)));
        let mut receipts = receipts(field);
        let mut wrong = full_raw(field, true);
        wrong["title"] = json!("Different actual observed value");
        receipts[child].stdout = serde_json::to_vec(&wrong).unwrap();
        receipts.truncate(child + 1);
        tail.extend(receipts);
        let (s, form, preview) = prepared(field, tail);
        let outcome = execute(&s, &form, &preview).unwrap();
        assert_eq!(outcome.state, State::Uncertain);
        assert!(outcome.effect.is_none());
        assert!(outcome.draft_retained());
        let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let target = target(&s, &form);
        assert_eq!(
            journal.blocking(&target).unwrap().unwrap().state,
            PrWriteState::Uncertain
        );
        let before = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        let saved = Cache::open(&database(&s.fixture))
            .unwrap()
            .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
            .unwrap();
        no_resend(&s, &form, &preview);
        assert_eq!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
                .unwrap(),
            saved
        );
        assert_eq!(
            fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
            before
        );
        exact(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_original_v25_actual_patch_errors_beside_data_are_not_receipt_and_body_ref_stay_protected(
) {
    let field = Field::Description;
    let mut tail = phase(full_raw(field, false), capability(json!(true)));
    let mut response = full_raw(field, true);
    response["errors"] = json!([{"message":"uncertain actual target"}]);
    let mut patch = receipts(field).remove(0);
    patch.stdout = serde_json::to_vec(&response).unwrap();
    tail.push(patch);
    let (s, form, preview) = prepared(field, tail);
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::Uncertain);
    let saved = Cache::open(&database(&s.fixture))
        .unwrap()
        .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
        .unwrap()
        .unwrap();
    let envelope: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(envelope["values"]["body"], REQUESTED);
    assert_eq!(
        envelope["publication"],
        serde_json::to_value(&outcome.reference).unwrap()
    );
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_original_v25_final_actual_uid_loss_retains_confirmed_receipt_without_private_page() {
    let field = Field::Title;
    let mut tail = paid(field);
    tail.last_mut().unwrap().stdout =
        serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    let (s, form, preview) = prepared(field, tail);
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::ConfirmedVisibilityUnavailable);
    assert!(outcome.refreshed_page_v19.is_none());
    assert!(outcome.return_v19.is_none());
    assert!(outcome.draft_retained());
    let saved = Cache::open(&database(&s.fixture))
        .unwrap()
        .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
        .unwrap()
        .expect("Failed final numeric UID must preserve the protected draft");
    assert_eq!(
        serde_json::from_str::<Value>(&saved).unwrap()["values"]["body"],
        REQUESTED
    );
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    assert_eq!(
        journal
            .original_publication(&target(&s, &form), &outcome.reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Confirmed
    );
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_original_v25_real_cleanup_sql_failure_keeps_confirmed_original_and_exact_body() {
    let field = Field::Description;
    // Cleanup is attempted only AFTER the same actual final numeric UID.
    let calls = paid(field);
    let (s, form, preview) = prepared(field, calls);
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER fail_metadata_cleanup_v25 BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'metadata-text-v25/%' BEGIN SELECT RAISE(ABORT,'actual metadata cleanup refuses'); END;").unwrap();
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::ConfirmedVisibilityUnavailable);
    assert!(outcome.draft_retained());
    let saved = Cache::open(&database(&s.fixture))
        .unwrap()
        .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&saved).unwrap()["values"]["body"],
        REQUESTED
    );
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    assert_eq!(
        journal
            .original_publication(&target(&s, &form), &outcome.reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Confirmed
    );
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_original_v25_real_form_edit_resize_close_revokes_exact_painted_request_without_new_children(
) {
    let (mut s, form, preview) = prepared(Field::Title, Vec::new());
    s.app.form = Some(form);
    s.app.overlay = Some(crate::app::Overlay::Form);
    let request = metadata::request(s.app.form.as_ref().unwrap()).unwrap();
    assert!(request.matches_confirm(s.app.form.as_ref().unwrap()));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 18)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, &mut s.app)).unwrap();
    assert!(!request.matches_confirm(s.app.form.as_ref().unwrap()));
    s.app.form.as_mut().unwrap().dispatch(&FormAction::Close);
    assert!(!request.matches_confirm(s.app.form.as_ref().unwrap()));
    assert!(!preview.used.load(std::sync::atomic::Ordering::Acquire));
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_original_v25_native_menu_has_typed_text_and_history_controls_no_legacy_compose_fallback(
) {
    let s = setup(Vec::new());
    let controls = s.app.pr_controls();
    for field in [Field::Title, Field::Description] {
        assert!(controls
            .iter()
            .any(|(_, a)| *a
                == crate::app::Action::Pr(crate::app::PrUiAction::MetadataTextV25(field))));
        assert!(controls.iter().any(|(_, a)| *a
            == crate::app::Action::Pr(crate::app::PrUiAction::MetadataHistoryV25(field))));
    }
    assert!(!controls.iter().any(|(_, a)| matches!(
        a,
        crate::app::Action::Github(crate::github_workspace::GithubAction::Compose(
            crate::forms::PrFormKind::Title | crate::forms::PrFormKind::Description
        ))
    )));
    unchanged(&s, s.steps.len());
}

fn unknown(tail: Vec<GhStep>) -> (Setup, Form, crate::pr_write_journal::PrPublicationReference) {
    let mut calls = phase(full_raw(Field::Description, false), capability(json!(true)));
    let mut patch = receipts(Field::Description).remove(0);
    patch.status = 1;
    patch.stdout.clear();
    patch.stderr = b"HTTP 403: actual metadata target refused".to_vec();
    calls.push(patch);
    calls.extend(tail);
    let (s, form, preview) = prepared(Field::Description, calls);
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::Uncertain);
    (s, form, outcome.reference)
}
fn history_phase() -> Vec<GhStep> {
    phase(full_raw(Field::Description, false), capability(json!(true)))
}
fn open_history(s: &Setup) -> Form {
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let cancel = original.cancellation();
    metadata::history::open(
        original.clone(),
        Field::Description,
        &bounded(s, &original, 1024 * 1024),
        &database(&s.fixture),
        cancel,
        original.deadline(),
        None,
    )
    .unwrap()
}
fn inspect(s: &Setup, form: &Form) -> Form {
    let request = metadata::history::request(form).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    metadata::history::open(
        request.view.original.clone(),
        Field::Description,
        &api,
        &database(&s.fixture),
        &cancel,
        deadline,
        Some(&request),
    )
    .unwrap()
}
fn paint_history(s: &mut Setup, form: Form) -> Form {
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(crate::app::Overlay::Form);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, &mut s.app)).unwrap();
    let form = s.app.form.take().unwrap();
    s.app.overlay = None;
    form
}
fn inspected(
    tail: Vec<GhStep>,
) -> (
    Setup,
    Form,
    Form,
    crate::pr_write_journal::PrPublicationReference,
) {
    let mut reads = history_phase();
    reads.extend(history_phase());
    reads.extend(tail);
    let (mut s, original, reference) = unknown(reads);
    let history = open_history(&s);
    let history = inspect(&s, &history);
    let history = paint_history(&mut s, history);
    assert!(metadata::history::ack_ready(&history));
    (s, original, history, reference)
}
#[test]
fn pr_metadata_original_v25_actual_unknown_history_fresh_inspection_exact_local_ack_keeps_body_reference_and_no_resend(
) {
    let (s, original, history, reference) = inspected(Vec::new());
    let saved = Cache::open(&database(&s.fixture))
        .unwrap()
        .bounded_draft(&original.scope(), &original.resource(), 128 * 1024)
        .unwrap();
    assert!(history.fields.is_empty());
    assert!(ui::encoded_draft(&history).is_err());
    let request = metadata::history::request(&history).unwrap();
    let current = request.view.current.as_ref().unwrap();
    let (actual, changed) = metadata::history::acknowledge(
        &request,
        &s.api,
        &database(&s.fixture),
        &current.cancel,
        current.deadline,
    )
    .unwrap();
    assert_eq!(actual, reference);
    assert_eq!(changed, 1);
    assert!(!metadata::history::ack_ready(&history));
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let target = target(&s, &original);
    assert!(journal.blocking(&target).unwrap().is_none());
    assert_eq!(
        journal
            .original_publication(&target, &reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Acknowledged
    );
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .bounded_draft(&original.scope(), &original.resource(), 128 * 1024)
            .unwrap(),
        saved
    );
    assert!(metadata::history::acknowledge(
        &request,
        &s.api,
        &database(&s.fixture),
        &current.cancel,
        current.deadline
    )
    .is_err());
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_original_v25_foreign_actual_runner_refuses_inspection_ack_without_children_or_released_sql(
) {
    let (s, original, history, reference) = inspected(Vec::new());
    let request = metadata::history::request(&history).unwrap();
    let current = request.view.current.as_ref().unwrap();
    let foreign = s
        .api
        .clone()
        .with_runner(crate::providers::runner::CommandRunner::default());
    assert!(metadata::history::acknowledge(
        &request,
        &foreign,
        &database(&s.fixture),
        &current.cancel,
        current.deadline
    )
    .is_err());
    assert!(!metadata::history::ack_ready(&history));
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    assert_eq!(
        journal
            .original_publication(&target(&s, &original), &reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Uncertain
    );
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_original_v25_foreign_token_and_cancelled_original_inspection_do_not_renew_clock_or_ack(
) {
    let (s, original, history, reference) = inspected(Vec::new());
    let request = metadata::history::request(&history).unwrap();
    let current = request.view.current.as_ref().unwrap();
    assert!(metadata::history::acknowledge(
        &request,
        &s.api,
        &database(&s.fixture),
        &CancellationToken::default(),
        current.deadline
    )
    .is_err());
    current.cancel.cancel();
    assert!(!metadata::history::ack_ready(&history));
    assert!(metadata::history::acknowledge(
        &request,
        &s.api,
        &database(&s.fixture),
        &current.cancel,
        current.deadline
    )
    .is_err());
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    assert_eq!(
        journal
            .original_publication(&target(&s, &original), &reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Uncertain
    );
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_original_v25_wrong_original_database_refuses_raw_ack_and_preserves_both_databases() {
    let (s, original, history, reference) = inspected(Vec::new());
    let request = metadata::history::request(&history).unwrap();
    let current = request.view.current.as_ref().unwrap();
    let other = s.fixture.root.join("other-cache.sqlite3");
    Cache::open(&other).unwrap();
    PrWriteJournal::open(&other).unwrap();
    assert!(metadata::history::acknowledge(
        &request,
        &s.api,
        &other,
        &current.cancel,
        current.deadline
    )
    .is_err());
    assert_eq!(
        rusqlite::Connection::open_with_flags(&other, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap()
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    assert_eq!(
        journal
            .original_publication(&target(&s, &original), &reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Uncertain
    );
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_original_v25_actual_raw_attempt_cas_mutation_refuses_local_ack_retaining_durable_blocker(
) {
    let (s, original, history, reference) = inspected(Vec::new());
    let request = metadata::history::request(&history).unwrap();
    let current = request.view.current.as_ref().unwrap();
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    assert_eq!(
        db.execute(
            "UPDATE pr_write_attempts SET created_at=created_at+1 WHERE attempt_id=?1",
            [reference.attempt_id]
        )
        .unwrap(),
        1
    );
    assert!(metadata::history::acknowledge(
        &request,
        &s.api,
        &database(&s.fixture),
        &current.cancel,
        current.deadline
    )
    .is_err());
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    assert_eq!(
        journal
            .original_publication(&target(&s, &original), &reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Uncertain
    );
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_original_v25_opposite_conversation_origin_graft_refuses_before_protected_body_admission(
) {
    let (s, original, reference) = unknown(history_phase());
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute("INSERT INTO pr_write_conversation_origins_v17(attempt_id,repository_id,pr_id,origin_json) VALUES(?1,10000,1007,'{}')",[reference.attempt_id]).unwrap();
    let opening = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    assert!(metadata::history::open(
        opening.clone(),
        Field::Description,
        &bounded(&s, &opening, 1024 * 1024),
        &database(&s.fixture),
        opening.cancellation(),
        opening.deadline(),
        None
    )
    .is_err());
    assert_eq!(
        db.query_row(
            "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
            [reference.attempt_id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "uncertain"
    );
    assert!(Cache::open(&database(&s.fixture))
        .unwrap()
        .bounded_draft(&original.scope(), &original.resource(), 128 * 1024)
        .unwrap()
        .is_some());
    exact(&s, s.steps.len() - 4);
}
#[test]
fn pr_metadata_original_v25_confirmed_cleanup_failure_fresh_history_is_labelled_confirmed_not_ack_or_editable_body(
) {
    let field = Field::Description;
    let mut tail = paid(field);
    tail.extend(phase(full_raw(field, true), capability(json!(true))));
    let (s, form, preview) = prepared(field, tail);
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER refuse_metadata_cleanup_history BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'metadata-text-v25/%' BEGIN SELECT RAISE(ABORT,'retain exact confirmed original'); END;").unwrap();
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::ConfirmedVisibilityUnavailable);
    assert!(outcome.draft_retained());
    let history = open_history(&s);
    let state = metadata::history::state(&history).unwrap();
    assert_eq!(
        state.view.snapshot.rows[0].marker.state,
        PrWriteState::Confirmed
    );
    assert!(state.view.lines.iter().any(|s| s.contains("Confirmed")));
    assert!(state.view.current.is_none());
    assert!(history.fields.is_empty());
    assert!(!metadata::history::ack_ready(&history));
    assert!(metadata::history::request(&history).is_err());
    exact(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_expired_inspection_cannot_ack_or_release_original() {
    let (s, original, history, reference) = inspected(Vec::new());
    let request = metadata::history::request(&history).unwrap();
    let current = request.view.current.as_ref().unwrap();
    assert!(metadata::history::acknowledge(
        &request,
        &s.api,
        &database(&s.fixture),
        &current.cancel,
        Instant::now() - Duration::from_millis(1)
    )
    .is_err());
    assert!(
        metadata::history::ack_ready(&history),
        "An expired caller deadline does not consume or renew original observation"
    );
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    assert_eq!(
        journal
            .original_publication(&target(&s, &original), &reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Uncertain
    );
    exact(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_external_draft_replacement_before_final_guard_refuses_post_and_retains_new_row(
) {
    let field = Field::Title;
    let mut calls = phase(full_raw(field, false), capability(json!(true)));
    calls[5].delay_ms = 2000;
    calls.truncate(7);
    let (s, form, preview) = prepared(field, calls);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| execute(&s, &form, &preview));
        let held = super::super::protocol::publication::refresh_v19::StoppedV19::final_uid(&s);
        Cache::open(&database(&s.fixture))
            .unwrap()
            .put_draft(
                &form.scope(),
                &form.resource(),
                "EXTERNAL_NEW_METADATA_ENVELOPE",
            )
            .unwrap();
        drop(held);
        let outcome = worker.join().unwrap().unwrap();
        assert_eq!(outcome.state, State::NotStartedProtected);
        assert!(outcome.draft_retained());
        assert_eq!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .draft(&form.scope(), &form.resource())
                .unwrap()
                .as_deref(),
            Some("EXTERNAL_NEW_METADATA_ENVELOPE")
        );
        assert_eq!(
            PrWriteJournal::open(&database(&s.fixture))
                .unwrap()
                .original_publication(&target(&s, &form), &outcome.reference)
                .unwrap()
                .unwrap()
                .state,
            PrWriteState::Pending
        );
    });
    exact(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_external_draft_replacement_after_receipt_never_deletes_newer_row() {
    let field = Field::Description;
    let mut calls = paid(field);
    calls.pop();
    calls.last_mut().unwrap().delay_ms = 2000;
    let (s, form, preview) = prepared(field, calls);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| execute(&s, &form, &preview));
        let held = super::super::protocol::publication::refresh_v19::StoppedV19::final_uid(&s);
        Cache::open(&database(&s.fixture))
            .unwrap()
            .put_draft(
                &form.scope(),
                &form.resource(),
                "EXTERNAL_POSTRECEIPT_METADATA_ENVELOPE",
            )
            .unwrap();
        drop(held);
        let outcome = worker.join().unwrap().unwrap();
        assert_eq!(outcome.state, State::ConfirmedVisibilityUnavailable);
        assert!(outcome.draft_retained());
        assert_eq!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .draft(&form.scope(), &form.resource())
                .unwrap()
                .as_deref(),
            Some("EXTERNAL_POSTRECEIPT_METADATA_ENVELOPE")
        );
        assert_eq!(
            PrWriteJournal::open(&database(&s.fixture))
                .unwrap()
                .original_publication(&target(&s, &form), &outcome.reference)
                .unwrap()
                .unwrap()
                .state,
            PrWriteState::Confirmed
        );
    });
    exact(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_newer_actual_same_runner_write_invalidates_inspected_ack_generation() {
    // I2 is a separate actual inspection, not a fabricated generation bump.
    let mut calls = history_phase();
    calls.extend(principal());
    calls.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    let mut write = get(
        &format!("repos/{REPO}/issues/7/comments"),
        json!({"id":900,"node_id":"IC_900","body":"later actual writer","user":{"id":42,"login":"octocat","type":"User"}}),
    );
    *write.args.iter_mut().find(|a| a.as_str() == "GET").unwrap() = "POST".into();
    write.args.extend(["--input".into(), "-".into()]);
    write.stdin = serde_json::to_vec(&json!({"body":"later actual writer"})).unwrap();
    calls.push(write);
    let (mut s, original, history, reference) = inspected(calls);
    let request = metadata::history::request(&history).unwrap();
    let current = request.view.current.as_ref().unwrap();
    let second = inspect(&s, &history);
    let second = paint_history(&mut s, second);
    let second_request = metadata::history::request(&second).unwrap();
    let second_current = second_request.view.current.as_ref().unwrap();
    assert!(!Arc::ptr_eq(current, second_current));
    assert!(metadata::history::ack_ready(&history));
    assert!(metadata::history::ack_ready(&second));
    let preview = metadata::state(&original)
        .unwrap()
        .preview
        .as_ref()
        .unwrap()
        .clone();
    no_resend(&s, &original, &preview);
    let stable = sql(&s.fixture);
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let before_marker = journal
        .original_publication(&target(&s, &original), &reference)
        .unwrap()
        .unwrap();
    let before_origin = journal
        .metadata_origin_v25(&target(&s, &original), &before_marker)
        .unwrap();
    let before_body = Cache::open(&database(&s.fixture))
        .unwrap()
        .bounded_draft(&original.scope(), &original.resource(), 128 * 1024)
        .unwrap()
        .unwrap();
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    let reservations = || {
        let mut statement = db.prepare("SELECT json_array(host,login,source,generation,active) FROM sync_reservations ORDER BY host,login,source").unwrap();
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        drop(statement);
        rows
    };
    let before_reservations = reservations();
    let trigger = "CREATE TRIGGER metadata_generation_ack_refusal_v42 BEFORE UPDATE OF state ON pr_write_attempts WHEN NEW.state='acknowledged' BEGIN SELECT RAISE(ABORT,'actual metadata ACK final SQL refuses'); END";
    db.execute_batch(trigger).unwrap();
    assert_eq!(db.query_row("SELECT sql FROM sqlite_master WHERE type='trigger' AND name='metadata_generation_ack_refusal_v42'",[],|row|row.get::<_,String>(0)).unwrap(), trigger);
    // Existing typed reconciliation runs BEFORE the SAME raw SQL CAS. The
    // trigger refuses only that final local ACK; it grants no native write.
    assert!(metadata::history::acknowledge(
        &second_request,
        &s.api,
        &database(&s.fixture),
        &second_current.cancel,
        second_current.deadline
    )
    .is_err());
    assert!(!metadata::history::ack_ready(&second));
    assert_eq!(sql(&s.fixture), stable);
    assert_eq!(reservations(), before_reservations);
    let after_marker = journal
        .original_publication(&target(&s, &original), &reference)
        .unwrap()
        .unwrap();
    assert_eq!(format!("{after_marker:?}"), format!("{before_marker:?}"));
    assert_eq!(
        serde_json::to_vec(
            &journal
                .metadata_origin_v25(&target(&s, &original), &after_marker)
                .unwrap()
        )
        .unwrap(),
        serde_json::to_vec(&before_origin).unwrap()
    );
    assert_eq!(after_marker.state, PrWriteState::Uncertain);
    assert_eq!(
        journal
            .blocking(&target(&s, &original))
            .unwrap()
            .unwrap()
            .publication_reference(),
        reference
    );
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .bounded_draft(&original.scope(), &original.resource(), 128 * 1024)
            .unwrap()
            .as_deref(),
        Some(before_body.as_str())
    );
    no_resend(&s, &original, &preview);
    db.execute_batch("DROP TRIGGER metadata_generation_ack_refusal_v42")
        .unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='metadata_generation_ack_refusal_v42'",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    assert!(
        current.validates_current_generation_v42(&s.api),
        "I2 typed reconcile plus SQL rollback must not invalidate I1's actual generation"
    );
    exact(&s, s.steps.len() - 4);
    let identity = s.app.github.as_ref().unwrap().identity.clone();
    assert_eq!(
        s.api
            .comment(
                &identity,
                REPO,
                7,
                &"a".repeat(40),
                "later actual writer",
                &CancellationToken::default()
            )
            .unwrap()
            .id,
        900
    );
    assert!(
        !current.validates_current_generation_v42(&s.api),
        "Only the genuine later same-runner write changes I1's observed generation"
    );
    assert_eq!(sql(&s.fixture), stable);
    assert_eq!(reservations(), before_reservations);
    no_resend(&s, &original, &preview);
    assert!(metadata::history::acknowledge(
        &request,
        &s.api,
        &database(&s.fixture),
        &current.cancel,
        current.deadline
    )
    .is_err());
    assert!(
        !metadata::history::ack_ready(&history),
        "Failed current-generation ACK is consumed; fresh Inspect required"
    );
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    assert_eq!(
        journal
            .original_publication(&target(&s, &original), &reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Uncertain
    );
    assert_eq!(sql(&s.fixture), stable);
    assert_eq!(reservations(), before_reservations);
    exact(&s, s.steps.len());
}

#[derive(Clone, Copy)]
enum LocalRefusalV38 {
    NoChange,
    Denied,
    WrongCapabilityNode,
}

fn native_local_refusal_v38(mode: LocalRefusalV38) {
    use crate::{
        app::{Action, Overlay, View},
        providers::gh::GithubErrorKind,
        runtime::{scoped_inline_test_support_v1 as channel, Background},
        scoped_pr_collaboration_native_v17 as native,
    };
    let denied = matches!(mode, LocalRefusalV38::Denied);
    let structural = matches!(mode, LocalRefusalV38::WrongCapabilityNode);
    let mut calls = phase(raw(json!(CURRENT)), capability(json!(!denied)));
    let mut observed = capability(json!(!denied));
    if structural {
        observed["data"]["node"]["id"] = json!("INDEPENDENT_OTHER_PR_NODE");
    }
    let mut refusal = phase(raw(json!(CURRENT)), observed);
    refusal.truncate(5); // Refuse before closing raw PR/repository/final UID.
    calls.extend(refusal.clone());
    calls.extend(refusal);
    let mut s = setup(calls);
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let mut form = metadata::open(
        original.clone(),
        Field::Description,
        &bounded(&s, &original, 1024 * 1024),
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    if !matches!(mode, LocalRefusalV38::NoChange) {
        ui::replace_editor_result(&mut form, REQUESTED).unwrap();
    }
    let request = metadata::request(&form).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let error = metadata::MetadataPreviewV25::prepare(
        &request,
        &api,
        &database(&s.fixture),
        &cancel,
        deadline,
    )
    .err()
    .expect("Actual same-Core producer must refuse this requested effect");
    assert!(match mode {
        LocalRefusalV38::NoChange => matches!(error.kind, GithubErrorKind::InvalidTarget),
        LocalRefusalV38::Denied => matches!(error.kind, GithubErrorKind::Permission),
        LocalRefusalV38::WrongCapabilityNode =>
            matches!(error.kind, GithubErrorKind::IdentityChanged),
    });
    if matches!(mode, LocalRefusalV38::NoChange) {
        assert_eq!(error.message, metadata::METADATA_NO_CHANGE_V65);
    }
    let page = allocations(&s.app);
    let ticket = metadata::state(&form).unwrap().ticket.clone();
    let intent = metadata::state(&form).unwrap().intent.clone();
    let presentation = form.presentation_v4().0.clone();
    let encoded = ui::encoded_draft(&form).unwrap();
    let mut writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    writer
        .save(form.scope(), form.resource(), encoded.clone())
        .unwrap();
    // save() accepts this producer's revision; it is not a durable ACK.
    // Pin that SAME accepted cutoff, then observe its actual successful save
    // before taking the full-SQL baseline used to judge the Preview worker.
    let saved_cancel = CancellationToken::default();
    let saved_deadline = Instant::now() + Duration::from_secs(8);
    let saved_cutoff = client
        .capture_conversation_open_cutoff_v20(&database(&s.fixture), &saved_cancel, saved_deadline)
        .unwrap();
    loop {
        let acknowledgements = writer.poll();
        if !acknowledgements.is_empty() {
            assert_eq!(acknowledgements.len(), 1);
            let acknowledged = acknowledgements.into_iter().next().unwrap();
            assert_eq!(acknowledged.scope, form.scope());
            assert_eq!(acknowledged.resource, form.resource());
            acknowledged.result.unwrap();
            break;
        }
        assert!(
            Instant::now() < saved_deadline,
            "Exact accepted Metadata draft save must acknowledge before baseline"
        );
        std::thread::yield_now();
    }
    client
        .wait_conversation_open_cutoff_v20(&saved_cutoff, &saved_cancel)
        .unwrap();
    saved_cutoff.check(&client, &saved_cancel).unwrap();
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
            .unwrap()
            .as_deref(),
        Some(encoded.as_str())
    );
    drop(saved_cutoff);
    let stable = sql(&s.fixture);
    let reservations = || {
        let db = rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let mut query = db
            .prepare("SELECT * FROM sync_reservations ORDER BY host,login,source")
            .unwrap();
        let columns = query.column_count();
        let rows = query
            .query_map([], |row| {
                (0..columns)
                    .map(|n| row.get::<_, rusqlite::types::Value>(n))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        drop(query);
        drop(db);
        rows
    };
    let before_reservations = reservations();
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(Overlay::Form);
    s.app.dispatch(Action::Form(FormAction::Preview));
    // The accepted Preview action itself advances the input epoch. The
    // worker must retain THAT current epoch, not resurrect an older request.
    let epoch = s.app.form.as_ref().unwrap().presentation_v4().1;
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert_eq!(work.phase(), native::ConversationPhaseKindV17::Preview);
    assert!(work.matches(&s.app));
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work.clone(),
        s.api.clone(),
        database(&s.fixture),
        client,
    ));
    let owner = s.app.scoped_pr_collaboration_owner_v17.clone().unwrap();
    assert!(Arc::ptr_eq(owner.work(), &work));
    assert!(owner.owns(&s.app));
    assert!(s.app.loading[View::Inbox.index()]);
    assert!(channel::generation(&background).is_some());
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Actual Metadata Preview worker did not settle"
        );
        std::thread::yield_now();
    }
    assert!(channel::generation(&background).is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert!(!owner.cancellation().is_cancelled());
    assert_eq!(sql(&s.fixture), stable);
    assert_eq!(reservations(), before_reservations);
    assert_eq!(
        rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        )
        .unwrap()
        .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
            .get::<_, u64>(0))
        .unwrap(),
        0,
    );
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .bounded_draft(original.scope(), &intent.key, 128 * 1024)
            .unwrap()
            .as_deref(),
        Some(encoded.as_str())
    );
    if structural {
        assert!(s.app.form.is_none());
        assert!(s.app.github.is_none());
        assert!(s.app.overlay.is_none());
        assert!(!owner.owns(&s.app));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut s.app))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!text.contains(CURRENT));
        assert!(!text.contains(REQUESTED));
        assert!(!text.contains(BODY));
    } else {
        assert_eq!(allocations(&s.app), page);
        assert_eq!(s.app.overlay, Some(Overlay::Form));
        assert!(!s.app.form_dismissed);
        let same = s.app.form.as_ref().unwrap();
        let state = metadata::state(same).unwrap();
        assert!(Arc::ptr_eq(&state.ticket, &ticket));
        assert!(Arc::ptr_eq(&state.intent, &intent));
        assert!(Arc::ptr_eq(same.presentation_v4().0, &presentation));
        assert_eq!(same.presentation_v4().1, epoch);
        assert_eq!(ui::encoded_draft(same).unwrap(), encoded);
        assert!(
            !same.preview && !same.submitting && !same.preview_requested && !same.submit_requested
        );
        assert!(same.publication.is_none());
        assert!(owner.owns(&s.app));
        assert_eq!(s.app.status.contains("Actual capability refused"), denied);
        if matches!(mode, LocalRefusalV38::NoChange) {
            assert_eq!(same.error, metadata::METADATA_NO_CHANGE_V65);
            assert_eq!(s.app.status, metadata::METADATA_NO_CHANGE_V65);
            assert!(state.preview.is_none());
            assert!(!metadata::request(same).unwrap().matches_confirm(same));
            for width in [80, 120] {
                let mut terminal =
                    ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, 40)).unwrap();
                terminal
                    .draw(|frame| crate::ui::draw(frame, &mut s.app))
                    .unwrap();
                let text = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>();
                assert!(text.contains(metadata::METADATA_NO_CHANGE_V65));
                assert!(text.contains("[Preview]"));
                assert!(text.contains("[Keep draft / Close]"));
                let form = s.app.form.as_ref().unwrap();
                assert!(Arc::ptr_eq(form.presentation_v4().0, &presentation));
                assert!(metadata::state(form).unwrap().preview.is_none());
                assert_eq!(ui::encoded_draft(form).unwrap(), encoded);
            }
            assert_eq!(sql(&s.fixture), stable);
            assert_eq!(reservations(), before_reservations);
        } else {
            assert_ne!(same.error, metadata::METADATA_NO_CHANGE_V65);
            assert_ne!(s.app.status, metadata::METADATA_NO_CHANGE_V65);
        }
    }
    exact(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_native_worker_nochange_retains_exact_form_and_draft_without_attempt() {
    native_local_refusal_v38(LocalRefusalV38::NoChange);
}

#[test]
fn pr_metadata_original_v25_native_worker_denied_retains_exact_form_and_draft_without_attempt() {
    native_local_refusal_v38(LocalRefusalV38::Denied);
}

#[test]
fn pr_metadata_original_v25_native_worker_wrong_capability_node_still_retires_private_presentation()
{
    native_local_refusal_v38(LocalRefusalV38::WrongCapabilityNode);
}

#[test]
fn pr_metadata_original_v25_invalid_title_preview_is_not_nochange_and_never_queues() {
    use crate::{
        app::{Action, Overlay, View},
        scoped_pr_collaboration_native_v17 as native,
    };
    let mut s = setup(phase(raw(json!(CURRENT)), capability(json!(true))));
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let mut form = metadata::open(
        original.clone(),
        Field::Title,
        &bounded(&s, &original, 1024 * 1024),
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    let page = allocations(&s.app);
    let ticket = metadata::state(&form).unwrap().ticket.clone();
    let presentation = form.presentation_v4().0.clone();
    let before_body = ui::encoded_draft(&form).unwrap();
    // The actual external-editor entry rejects oversized bytes without
    // mutating the original field or producing a nochange worker result.
    assert!(ui::replace_editor_result(&mut form, &"x".repeat(65_537)).is_err());
    assert_eq!(ui::encoded_draft(&form).unwrap(), before_body);
    ui::replace_editor_result(&mut form, " \t").unwrap();
    assert!(metadata::request(&form).is_err());
    let encoded = ui::encoded_draft(&form).unwrap();
    let stable = sql(&s.fixture);
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(Overlay::Form);
    s.app.dispatch(Action::Form(FormAction::Preview));
    native::prepare_requested(&mut s.app);
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    assert!(!s.app.loading[View::Inbox.index()]);
    assert_eq!(allocations(&s.app), page);
    assert_eq!(s.app.overlay, Some(Overlay::Form));
    let same = s.app.form.as_ref().unwrap();
    assert!(Arc::ptr_eq(same.presentation_v4().0, &presentation));
    assert!(Arc::ptr_eq(&metadata::state(same).unwrap().ticket, &ticket));
    assert!(!same.error.is_empty());
    assert_ne!(same.error, metadata::METADATA_NO_CHANGE_V65);
    assert_ne!(s.app.status, metadata::METADATA_NO_CHANGE_V65);
    assert!(!same.preview && !same.submitting && same.publication.is_none());
    assert!(metadata::state(same).unwrap().preview.is_none());
    assert_eq!(ui::encoded_draft(same).unwrap(), encoded);
    assert_eq!(sql(&s.fixture), stable);
    exact(&s, s.steps.len());
}

fn paint_form_v44(app: &mut App, width: u16, height: u16) {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
}

fn reservations_v44(s: &Setup) -> Vec<Vec<rusqlite::types::Value>> {
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = db
        .prepare("SELECT * FROM sync_reservations ORDER BY host,login,source")
        .unwrap();
    let columns = statement.column_count();
    let rows = statement
        .query_map([], |row| {
            (0..columns)
                .map(|index| row.get(index))
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    drop(statement);
    rows
}

fn key_v44(app: &mut App, code: crossterm::event::KeyCode) {
    app.handle_event(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::empty()),
    ));
}

fn save_editor_draft_v67(s: &Setup, form: &Form) -> DraftWriter {
    let mut writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    let encoded = ui::encoded_draft(form).unwrap();
    writer
        .save(form.scope(), form.resource(), encoded.clone())
        .unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(8);
    let cutoff = client
        .capture_conversation_open_cutoff_v20(&database(&s.fixture), &cancel, deadline)
        .unwrap();
    loop {
        let acknowledgements = writer.poll();
        if !acknowledgements.is_empty() {
            assert_eq!(acknowledgements.len(), 1);
            let acknowledged = acknowledgements.into_iter().next().unwrap();
            assert_eq!(acknowledged.scope, form.scope());
            assert_eq!(acknowledged.resource, form.resource());
            acknowledged.result.unwrap();
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Exact editor draft save must acknowledge"
        );
        std::thread::yield_now();
    }
    client
        .wait_conversation_open_cutoff_v20(&cutoff, &cancel)
        .unwrap();
    cutoff.check(&client, &cancel).unwrap();
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
            .unwrap()
            .as_deref(),
        Some(encoded.as_str())
    );
    writer
}

#[test]
fn pr_metadata_original_v25_keyboard_and_mouse_editor_retires_title_and_description_preview() {
    use crate::{
        app::{Action, Overlay, View},
        scoped_pr_collaboration_native_v17 as native,
    };
    use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    for field in [Field::Title, Field::Description] {
        for mouse in [false, true] {
            let (mut s, form, preview) = prepared(field, vec![]);
            let _writer = save_editor_draft_v67(&s, &form);
            let encoded = ui::encoded_draft(&form).unwrap();
            let stable = sql(&s.fixture);
            let reservations = reservations_v44(&s);
            let page = allocations(&s.app);
            let ticket = metadata::state(&form).unwrap().ticket.clone();
            let intent = metadata::state(&form).unwrap().intent.clone();
            let presentation = form.presentation_v4().0.clone();
            s.app.form = Some(form);
            s.app.form_dismissed = false;
            s.app.overlay = Some(Overlay::Form);
            paint_form_v44(&mut s.app, 120, 40);
            if !mouse {
                assert_eq!(s.app.form.as_ref().unwrap().focus, 1);
                key_v44(&mut s.app, KeyCode::Tab);
                paint_form_v44(&mut s.app, 120, 40);
                assert_eq!(s.app.form.as_ref().unwrap().focus, 2);
            }
            // Capture AFTER keyboard focus movement: it is the Editor action,
            // not an earlier Tab epoch, which must now retire this approval.
            let old = metadata::request(s.app.form.as_ref().unwrap()).unwrap();
            assert!(old.matches_confirm(s.app.form.as_ref().unwrap()));
            let epoch = s.app.form.as_ref().unwrap().presentation_v4().1;
            let revision = metadata::state(s.app.form.as_ref().unwrap())
                .unwrap()
                .revision;
            if mouse {
                let rect = s
                    .app
                    .hits
                    .iter()
                    .rev()
                    .find(|hit| matches!(hit.action, Action::Form(FormAction::Editor)))
                    .expect("Actual metadata Preview exposes External editor")
                    .rect;
                s.app.handle_event(Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rect.x,
                    row: rect.y,
                    modifiers: KeyModifiers::empty(),
                }));
            } else {
                key_v44(&mut s.app, KeyCode::Enter);
            }
            let same = s.app.form.as_mut().unwrap();
            assert!(same.editor_requested);
            assert!(
                !same.preview
                    && !same.preview_requested
                    && !same.submit_requested
                    && !same.submitting
            );
            assert!(same.publication.is_none());
            assert!(metadata::state(same).unwrap().preview.is_none());
            assert_ne!(same.presentation_v4().1, epoch);
            assert_ne!(metadata::state(same).unwrap().revision, revision);
            assert!(Arc::ptr_eq(same.presentation_v4().0, &presentation));
            assert!(Arc::ptr_eq(&metadata::state(same).unwrap().ticket, &ticket));
            assert!(Arc::ptr_eq(&metadata::state(same).unwrap().intent, &intent));
            assert!(!old.matches(same) && !old.matches_confirm(same));
            assert!(!preview.used.load(std::sync::atomic::Ordering::Acquire));
            assert!(ui::validate_input(same, "").is_ok());
            assert!(ui::confirmed_preview(same).is_err());
            assert_eq!(ui::encoded_draft(same).unwrap(), encoded);

            // Main consumes the request before launching the process. A
            // canceled/failed editor supplies NO replacement: this boundary
            // still has the original text and no usable old approval. This
            // code contract does not pretend to launch an editor process.
            same.editor_requested = false;
            assert_eq!(ui::encoded_draft(same).unwrap(), encoded);
            assert!(!old.matches_confirm(same));
            assert!(ui::replace_editor_result(same, &"x".repeat(65_537)).is_err());
            assert_eq!(ui::encoded_draft(same).unwrap(), encoded);
            assert!(metadata::state(same).unwrap().preview.is_none());
            let replacement = if field == Field::Title {
                "Actual external title café 雪"
            } else {
                "Actual external description café 雪\nsecond exact line"
            };
            ui::replace_editor_result(same, replacement).unwrap();
            assert_eq!(same.fields[0].editor.text, replacement);
            assert!(same.dirty && !same.preview);
            assert!(!old.matches_confirm(same));
            assert!(!metadata::request(same).unwrap().matches_confirm(same));
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut s.app))
                .unwrap();
            // Disabled controls intentionally retain their actual click hit
            // so activation can explain the refusal. Verify the current
            // rendered muted control AND its guarded input semantics.
            let confirm = s
                .app
                .hits
                .iter()
                .rev()
                .find(|hit| matches!(hit.action, Action::Form(FormAction::Submit)))
                .expect("Actual unapproved Intent keeps its disabled Confirm control")
                .rect;
            let same = s.app.form.as_ref().unwrap();
            assert!(!same.plan_ready());
            assert!(ui::confirmed_preview(same).is_err());
            assert_eq!(same.focus, same.fields.len() + usize::from(!mouse));
            let buffer = terminal.backend().buffer();
            let label = (confirm.x..confirm.right())
                .map(|x| buffer[(x, confirm.y)].symbol())
                .collect::<String>();
            assert_eq!(label, "[Confirm action]");
            let muted = s.app.theme().style(crate::theme::SemanticToken::Muted);
            for x in confirm.x..confirm.right() {
                let cell = &buffer[(x, confirm.y)];
                assert_eq!(cell.fg, muted.fg.unwrap_or(ratatui::style::Color::Reset));
                assert_eq!(cell.bg, muted.bg.unwrap_or(ratatui::style::Color::Reset));
                assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
                assert!(!cell.modifier.contains(ratatui::style::Modifier::REVERSED));
            }
            if mouse {
                s.app.handle_event(Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: confirm.x,
                    row: confirm.y,
                    modifiers: KeyModifiers::empty(),
                }));
            } else {
                key_v44(&mut s.app, KeyCode::Tab);
                paint_form_v44(&mut s.app, 120, 40);
                let same = s.app.form.as_ref().unwrap();
                assert_eq!(same.focus, same.fields.len() + 2);
                key_v44(&mut s.app, KeyCode::Enter);
            }
            let same = s.app.form.as_ref().unwrap();
            assert!(!same.submit_requested && !same.preview_requested && !same.editor_requested);
            assert!(!same.preview && !same.error.is_empty());
            assert!(!old.matches_confirm(same));
            assert!(metadata::state(same).unwrap().preview.is_none());
            assert_eq!(same.fields[0].editor.text, replacement);
            assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
            assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
            s.app.dispatch(Action::Form(FormAction::Submit));
            assert!(!s.app.form.as_ref().unwrap().submit_requested);
            native::prepare_requested(&mut s.app);
            assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
            assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
            assert!(!s.app.loading[View::Inbox.index()]);
            assert_eq!(allocations(&s.app), page);
            assert_eq!(sql(&s.fixture), stable);
            assert_eq!(reservations_v44(&s), reservations);
            assert_eq!(
                Cache::open(&database(&s.fixture))
                    .unwrap()
                    .bounded_draft(intent.source.opening().scope(), &intent.key, 128 * 1024)
                    .unwrap()
                    .as_deref(),
                Some(encoded.as_str())
            );
            exact(&s, s.steps.len());
        }
    }
}

#[test]
fn pr_metadata_original_v25_editor_preserves_busy_pending_and_malformed_original_inputs() {
    for field in [Field::Title, Field::Description] {
        let (s, mut form, preview) = prepared(field, vec![]);
        let stable = sql(&s.fixture);
        let reservations = reservations_v44(&s);
        let original_fields = form.fields.clone();
        let original_epoch = form.presentation_v4().1;
        let original_revision = metadata::state(&form).unwrap().revision;
        for mode in [
            "submitting",
            "preview",
            "submit",
            "editor",
            "oversize",
            "wrong-key",
            "extra-field",
        ] {
            form.fields = original_fields.clone();
            form.submitting = mode == "submitting";
            form.preview_requested = mode == "preview";
            form.submit_requested = mode == "submit";
            form.editor_requested = mode == "editor";
            if mode == "oversize" {
                form.fields[0].editor = crate::editor::TextBuffer::new("x".repeat(65_537)).unwrap();
            } else if mode == "wrong-key" {
                form.fields[0].key = "not-the-original-body".into();
            } else if mode == "extra-field" {
                form.fields
                    .push(crate::forms::Field::text("foreign", "Foreign", "value", false).unwrap());
            }
            let fields = form.values();
            let flags = (
                form.submitting,
                form.preview_requested,
                form.submit_requested,
                form.editor_requested,
            );
            form.dispatch(&FormAction::Editor);
            assert_eq!(
                (
                    form.submitting,
                    form.preview_requested,
                    form.submit_requested,
                    form.editor_requested
                ),
                flags,
                "{mode}"
            );
            assert_eq!(form.values(), fields, "{mode}");
            assert!(form.preview);
            assert_eq!(form.presentation_v4().1, original_epoch);
            assert_eq!(metadata::state(&form).unwrap().revision, original_revision);
            assert!(Arc::ptr_eq(
                metadata::state(&form).unwrap().preview.as_ref().unwrap(),
                &preview
            ));
            assert!(!preview.used.load(std::sync::atomic::Ordering::Acquire));
            assert_eq!(sql(&s.fixture), stable);
            assert_eq!(reservations_v44(&s), reservations);
        }
        exact(&s, s.steps.len());
    }
}

#[test]
fn pr_metadata_original_v25_editor_can_correct_blank_title_without_inventing_preview() {
    let (s, mut form, _) = prepared(Field::Title, vec![]);
    ui::replace_editor_result(&mut form, " ").unwrap_err();
    // Replacement while approved is refused. Explicit field editing first
    // invalidates the plan, and an invalid title must remain editor-correctable.
    form.invalidate_plan();
    ui::replace_editor_result(&mut form, " \t").unwrap();
    assert!(metadata::request(&form).is_err());
    let stable = sql(&s.fixture);
    form.dispatch(&FormAction::Editor);
    assert!(form.editor_requested && !form.preview);
    assert!(ui::validate_input(&form, "").is_ok());
    form.editor_requested = false;
    ui::replace_editor_result(&mut form, "Corrected genuine title").unwrap();
    assert!(metadata::request(&form).is_ok());
    assert!(!metadata::request(&form).unwrap().matches_confirm(&form));
    assert_eq!(sql(&s.fixture), stable);
    exact(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_editor_never_admits_actual_base_or_protected_history_modes() {
    let mut calls = phase(raw(json!(CURRENT)), capability(json!(true)));
    calls.insert(
        5,
        get(
            &format!("repos/{REPO}/branches?per_page=100&page=1"),
            json!([
                {"name":"main","commit":{"sha":"b".repeat(40)},"protected":false}
            ]),
        ),
    );
    let s = setup(calls);
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let mut base = ui::metadata_base_v32::open(
        original.clone(),
        &bounded(&s, &original, 1024 * 1024),
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    let before = base.values();
    let stable = sql(&s.fixture);
    let epoch = base.presentation_v4().1;
    base.dispatch(&FormAction::Editor);
    assert!(!base.editor_requested && !base.preview_requested && !base.submit_requested);
    assert_eq!(base.values(), before);
    assert_eq!(base.presentation_v4().1, epoch);
    assert!(ui::validate_input(&base, "").is_err());
    assert_eq!(sql(&s.fixture), stable);
    exact(&s, s.steps.len());

    let (s, original_form, reference) = unknown(history_phase());
    let mut history = open_history(&s);
    let request = metadata::history::request(&history).unwrap();
    let stable = sql(&s.fixture);
    let epoch = history.presentation_v4().1;
    history.dispatch(&FormAction::Editor);
    assert!(!history.editor_requested && !history.preview_requested && !history.submit_requested);
    assert_eq!(history.presentation_v4().1, epoch);
    assert!(history.fields.is_empty());
    assert!(Arc::ptr_eq(
        &metadata::history::request(&history).unwrap().view,
        &request.view
    ));
    assert!(ui::validate_input(&history, "").is_err());
    assert_eq!(
        PrWriteJournal::open(&database(&s.fixture))
            .unwrap()
            .original_publication(&target(&s, &original_form), &reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Uncertain
    );
    assert_eq!(sql(&s.fixture), stable);
    exact(&s, s.steps.len());

    editor_refuses_actual_terminal_return_v67();
}

fn editor_refuses_actual_terminal_return_v67() {
    use crate::{
        app::{Action, Overlay, View},
        runtime::{scoped_inline_test_support_v1 as channel, Background},
        scoped_pr_collaboration_native_v17 as native,
    };
    let field = Field::Description;
    let calls = paid(field); // The genuine final UID precedes cleanup failure.
    let (mut s, form, _) = prepared(field, calls);
    let writer = save_editor_draft_v67(&s, &form);
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER retain_editor_terminal_v67 BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'metadata-text-v25/%' BEGIN SELECT RAISE(ABORT,'retain actual editor terminal draft'); END;").unwrap();
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(Overlay::Form);
    paint_form_v44(&mut s.app, 120, 40);
    confirm_input_v44(&mut s.app, false);
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(work.matches(&s.app));
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client(),
    ));
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Actual cleanup-failed terminal must settle"
        );
        std::thread::yield_now();
    }
    assert!(channel::generation(&background).is_none());
    let descriptor = ui::return_descriptor_v19(s.app.form.as_ref().unwrap())
        .unwrap()
        .clone();
    let epoch = s.app.form.as_ref().unwrap().presentation_v4().1;
    let stable = sql(&s.fixture);
    let reservations = reservations_v44(&s);
    s.app.dispatch(Action::Form(FormAction::Editor));
    let same = s.app.form.as_ref().unwrap();
    assert!(Arc::ptr_eq(
        ui::return_descriptor_v19(same).unwrap(),
        &descriptor
    ));
    assert_eq!(same.presentation_v4().1, epoch);
    assert!(!same.editor_requested && !same.preview_requested && !same.submit_requested);
    assert!(ui::validate_input(same, "").is_err());
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    assert_eq!(sql(&s.fixture), stable);
    assert_eq!(reservations_v44(&s), reservations);
    exact(&s, s.steps.len());
}

fn confirm_input_v44(app: &mut App, mouse: bool) {
    use crate::app::Action;
    use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    if mouse {
        let rect = app
            .hits
            .iter()
            .rev()
            .find(|hit| matches!(hit.action, Action::Form(FormAction::Submit)))
            .expect("Actual useful Metadata Form paints an enabled Confirm hit")
            .rect;
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::empty(),
        }));
    } else {
        let form = app.form.as_ref().unwrap();
        assert_eq!(form.focus, form.fields.len());
        for _ in 0..2 {
            key_v44(app, KeyCode::Tab);
            paint_form_v44(app, 120, 40);
        }
        let form = app.form.as_ref().unwrap();
        assert_eq!(form.focus, form.fields.len() + 2);
        key_v44(app, KeyCode::Enter);
    }
}

fn confirmed_background_v44(mouse: bool, field: Field) {
    use crate::{
        app::{Overlay, View},
        runtime::{scoped_inline_test_support_v1 as channel, Background},
        scoped_pr_collaboration_native_v17 as native,
    };
    let (mut s, form, preview) = prepared(field, paid(field));
    let target = target(&s, &form);
    let key = form.resource();
    let presentation = form.presentation_v4().0.clone();
    let ticket = metadata::state(&form).unwrap().ticket.clone();
    let before = sql(&s.fixture);
    let mut writer = DraftWriter::new(database(&s.fixture));
    writer
        .save(form.scope(), key.clone(), ui::encoded_draft(&form).unwrap())
        .unwrap();
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(Overlay::Form);
    paint_form_v44(&mut s.app, 120, 40);
    confirm_input_v44(&mut s.app, mouse);
    let form = s.app.form.as_ref().unwrap();
    assert!(form.submit_requested && form.error.is_empty());
    assert!(Arc::ptr_eq(form.presentation_v4().0, &presentation));
    assert!(Arc::ptr_eq(&metadata::state(form).unwrap().ticket, &ticket));
    assert!(Arc::ptr_eq(
        metadata::state(form).unwrap().preview.as_ref().unwrap(),
        &preview
    ));
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert_eq!(work.phase(), native::ConversationPhaseKindV17::Confirm);
    assert!(work.matches(&s.app));
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work.clone(),
        s.api.clone(),
        database(&s.fixture),
        writer.client(),
    ));
    let owner = s.app.scoped_pr_collaboration_owner_v17.clone().unwrap();
    assert!(Arc::ptr_eq(owner.work(), &work));
    assert!(owner.owns(&s.app));
    assert!(s.app.loading[View::Inbox.index()]);
    assert!(channel::generation(&background).is_some());
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Actual Metadata Confirm worker did not settle"
        );
        std::thread::yield_now();
    }
    assert!(channel::generation(&background).is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert!(s.app.form.is_none());
    assert_eq!(s.app.overlay, None);
    assert!(!owner.cancellation().is_cancelled());
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT state FROM pr_write_attempts", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "confirmed"
    );
    assert!(PrWriteJournal::open(&database(&s.fixture))
        .unwrap()
        .blocking(&target)
        .unwrap()
        .is_none());
    let cache = Cache::open(&database(&s.fixture)).unwrap();
    assert!(cache.draft(target.scope(), &key).unwrap().is_none());
    let record = cache
        .get(target.scope(), "pr-details", target.node_id())
        .unwrap()
        .unwrap();
    assert_eq!(
        record.payload["summary"]["pr"][if field == Field::Title {
            "title"
        } else {
            "body"
        }],
        REQUESTED
    );
    assert_ne!(sql(&s.fixture), before);
    exact(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_keyboard_confirm_dispatch_reaches_actual_background_patch_and_fresh_page(
) {
    confirmed_background_v44(false, Field::Title);
}

#[test]
fn pr_metadata_original_v25_mouse_confirm_dispatch_reaches_actual_background_patch_and_fresh_page()
{
    confirmed_background_v44(true, Field::Description);
}

#[test]
fn pr_metadata_original_v25_unpainted_and_queued_edit_resize_close_refuse_before_fence_or_queue() {
    use crate::{
        app::{Action, Overlay, View},
        runtime::{scoped_inline_test_support_v1 as channel, Background},
        scoped_pr_collaboration_native_v17 as native,
    };
    use crossterm::event::{Event, KeyCode};
    for mode in ["unpainted", "edited", "resize", "closed"] {
        let (mut s, form, _) =
            prepared_display_v44(Field::Description, vec![], mode != "unpainted");
        let before = sql(&s.fixture);
        let reservations = reservations_v44(&s);
        let page = allocations(&s.app);
        let ticket = metadata::state(&form).unwrap().ticket.clone();
        let presentation = form.presentation_v4().0.clone();
        s.app.form = Some(form);
        s.app.form_dismissed = false;
        s.app.overlay = Some(Overlay::Form);
        let mut background = Background::new();
        let writer = DraftWriter::new(database(&s.fixture));
        if mode == "unpainted" {
            // There is deliberately no rendered clickable Confirm. Exercise
            // Form.dispatch's closed defense without inventing a native hit.
            s.app.dispatch(Action::Form(FormAction::Submit));
            let same = s.app.form.as_ref().unwrap();
            assert!(!same.submit_requested);
            assert!(same
                .error
                .contains("metadata Preview required before Confirm"));
            native::prepare_requested(&mut s.app);
        } else {
            paint_form_v44(&mut s.app, 120, 40);
            confirm_input_v44(&mut s.app, false);
            native::prepare_requested(&mut s.app);
            let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
            assert!(work.matches(&s.app));
            match mode {
                "edited" => {
                    s.app.dispatch(Action::Form(FormAction::Preview));
                    paint_form_v44(&mut s.app, 120, 40);
                    s.app.dispatch(Action::Form(FormAction::Field(0)));
                    s.app
                        .handle_event(Event::Paste(" changed after accepted Confirm".into()));
                    assert!(s.app.form.as_ref().unwrap().fields[0]
                        .editor
                        .text
                        .ends_with(" changed after accepted Confirm"));
                }
                "resize" => s.app.handle_event(Event::Resize(40, 10)),
                "closed" => key_v44(&mut s.app, KeyCode::Esc),
                _ => unreachable!(),
            }
            assert!(!work.matches(&s.app));
            assert!(!background.submit_scoped_pr_collaboration_v17(
                &mut s.app,
                work.clone(),
                s.api.clone(),
                database(&s.fixture),
                writer.client()
            ));
            assert!(!work.original().cancellation().is_cancelled());
        }
        assert_eq!(allocations(&s.app), page);
        if mode != "closed" {
            let same = s.app.form.as_ref().unwrap();
            assert!(Arc::ptr_eq(&metadata::state(same).unwrap().ticket, &ticket));
            assert!(Arc::ptr_eq(same.presentation_v4().0, &presentation));
            assert!(!same.submitting && !same.submit_requested);
        } else {
            assert!(s.app.form_dismissed && s.app.overlay.is_none());
        }
        assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
        assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
        assert!(!s.app.loading[View::Inbox.index()]);
        assert!(channel::generation(&background).is_none());
        assert_eq!(sql(&s.fixture), before);
        assert_eq!(reservations_v44(&s), reservations);
        exact(&s, s.steps.len());
    }
}

#[test]
fn pr_metadata_original_v25_reopened_actual_protected_reference_has_no_confirm_hit_or_dispatch() {
    use crate::app::{Action, Overlay};
    let mut calls = paid(Field::Description);
    calls.extend(history_phase());
    let (mut s, original_form, preview) = prepared(Field::Description, calls);
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER retain_confirmed_metadata_v44 BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'metadata-text-v25/%' BEGIN SELECT RAISE(ABORT,'retain actual confirmed original'); END;").unwrap();
    let outcome = execute(&s, &original_form, &preview).unwrap();
    assert_eq!(outcome.state, State::ConfirmedVisibilityUnavailable);
    assert!(outcome.draft_retained());
    let reference = outcome.reference;
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    assert_eq!(
        journal
            .original_publication(&target(&s, &original_form), &reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Confirmed
    );
    assert!(journal
        .blocking(&target(&s, &original_form))
        .unwrap()
        .is_none());
    db.execute_batch("DROP TRIGGER retain_confirmed_metadata_v44")
        .unwrap();
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let form = metadata::open(
        original.clone(),
        Field::Description,
        &bounded(&s, &original, 1024 * 1024),
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    assert_eq!(form.publication.as_ref(), Some(&reference));
    let before = sql(&s.fixture);
    let reservations = reservations_v44(&s);
    let encoded = ui::encoded_draft(&form).unwrap();
    let ticket = metadata::state(&form).unwrap().ticket.clone();
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(Overlay::Form);
    paint_form_v44(&mut s.app, 120, 40);
    assert!(!s
        .app
        .hits
        .iter()
        .any(|hit| matches!(hit.action, Action::Form(FormAction::Submit))));
    s.app.dispatch(Action::Form(FormAction::Submit));
    crate::scoped_pr_collaboration_native_v17::prepare_requested(&mut s.app);
    s.app.dispatch(Action::Form(FormAction::Editor));
    let same = s.app.form.as_ref().unwrap();
    assert_eq!(same.publication.as_ref(), Some(&reference));
    assert!(Arc::ptr_eq(&metadata::state(same).unwrap().ticket, &ticket));
    assert_eq!(ui::encoded_draft(same).unwrap(), encoded);
    assert!(!same.submitting && !same.submit_requested && !same.editor_requested);
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    assert_eq!(sql(&s.fixture), before);
    assert_eq!(reservations_v44(&s), reservations);
    exact(&s, s.steps.len());
}
