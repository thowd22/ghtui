//! DECLARED / UNRUN: actual original Files/source factories, real SQLite and
//! ordered writer. These do not substitute for the paired native journeys.
use super::*;
#[path = "scoped_pr_file_comment_capture_v82_tests.rs"]
mod capture_v82;
use crate::{
    drafts::DraftWriter,
    pr_write_journal::{
        file_comment_history_v75::FileCommentOriginSeedV75, PrWriteJournal, PrWriteTarget,
    },
    scoped_pr_observed_tabs_v1::resources::threads_v1::{
        file_comments_v75 as file, history_v1 as history,
    },
};
fn observation() -> Vec<GhStep> {
    let mut steps = publish_opening();
    steps.extend(last_guard());
    steps.extend(close_phase());
    assert_eq!(steps.len(), 11);
    steps
}
fn history_phase(uid: u64) -> Vec<GhStep> {
    let mut steps = principal(uid, "actual-history-inspector");
    if uid != 42 {
        return steps;
    }
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.extend(principal(42, "renamed-final-history-inspector"));
    assert_eq!(steps.len(), 8);
    steps
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
// Fault injection alone needs writes. Preserve the inherited read-only SQL
// observer for independent assertions and never create a missing fixture DB.
fn writable_fixture_sql(fixture: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap()
}
fn summary(real: &mut Real) {
    real.app.overlay = None;
    real.app.inbox_list.selected = 0;
    real.app.inbox_list.top = 0;
    let github = real.app.github.as_mut().unwrap();
    github.scoped_pr_resource = None;
    github.pr.as_mut().unwrap().tab = crate::github_workspace::PrTab::Summary;
    github.pr.as_mut().unwrap().prepare_rows().unwrap();
}
fn exact_protocol(real: &Real, writes: usize) {
    assert_eq!(git_state_resource(&real.fixture), real.git);
    let rows: Vec<Value> = fs::read_to_string(real.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    assert_eq!(rows.len(), real.steps.len());
    let mut actual_writes = 0;
    for (row, step) in rows.iter().zip(&real.steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        if step.args.iter().any(|a| a == "POST") && step.args[1] != "graphql" {
            actual_writes += 1;
            assert_eq!(step.args[1], format!("repos/{REPO}/pulls/7/comments"));
            let payload: Value = serde_json::from_slice(&step.stdin).unwrap();
            assert_eq!(payload["subject_type"], "file");
            assert_eq!(payload.as_object().unwrap().len(), 4);
            for forbidden in ["line", "side", "start_line", "position", "in_reply_to"] {
                assert!(payload.get(forbidden).is_none());
            }
        }
    }
    assert_eq!(actual_writes, writes);
}
#[test]
fn file_comments_v75_intent_captures_real_member_without_gh_and_rejects_changed_selection_or_uid() {
    let mut real = real(Vec::new());
    let intent = file::FileCommentIntentV75::capture(&real.app).unwrap();
    assert_eq!(intent.path(), "src/view.rs");
    assert!(intent.owns(&real.app));
    real.app.inbox_list.selected += 1;
    assert!(!intent.owns(&real.app));
    real.app.inbox_list.selected -= 1;
    real.app.github.as_mut().unwrap().identity.account_id = Some(43);
    assert!(!intent.owns(&real.app));
    intact(&real);
}
#[test]
fn file_comments_v75_genuine_form_bounds_busy_input_and_same_text_editor_retire_approval() {
    let real = real(observation());
    let intent = file::FileCommentIntentV75::capture(&real.app).unwrap();
    let draft = file::FileCommentDraftV75::open(
        intent,
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        deadline(),
    )
    .unwrap();
    assert_eq!(draft.identity().account_id, Some(42));
    let mut form = file::form::form_from_draft(draft).unwrap();
    form.paste(FILE_BODY);
    let request = file::form::request(&form).unwrap();
    form.submitting = true;
    form.paste("BUSY_POISON");
    assert!(request.matches_form(&form));
    form.submitting = false;
    assert!(file::form::validate_input(&form, &"x".repeat(65_537)).is_err());
    assert!(file::form::replace_editor_result(&mut form, &"界".repeat(65_536)).is_err());
    file::form::replace_editor_result(&mut form, FILE_BODY).unwrap();
    assert!(!request.matches_form(&form));
    assert_eq!(form.fields[0].editor.text, FILE_BODY);
    intact(&real);
}
#[test]
fn file_comments_v75_exact_prepared_doc_and_escaped_raw_editor_body_are_installed_only_for_matching_request(
) {
    let mut steps = observation();
    steps.extend(observation());
    let real = real(steps);
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
    let body = "Whole FILE \u{1b}[2J 界";
    file::form::replace_editor_result(&mut form, body).unwrap();
    let request = file::form::request(&form).unwrap();
    let preview = file::FileCommentPreviewV75::prepare(
        request.intent().clone(),
        request.body().clone(),
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        deadline(),
    )
    .unwrap();
    assert_eq!(preview.body(), body);
    assert!(preview.lines().iter().any(|line| line.contains("FILE")));
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains("\\u{1b}[2J")));
    assert!(!preview.lines().iter().any(|line| line.contains('\u{1b}')));
    assert!(file::form::accept_preview(
        &mut form,
        &request,
        preview.clone()
    ));
    assert!(Arc::ptr_eq(
        file::form::request(&form).unwrap().preview().unwrap(),
        &preview
    ));
    let approved = file::form::request(&form).unwrap();
    form.paste("READ_ONLY_PREVIEW_POISON");
    assert!(approved.matches_form(&form));
    intact(&real);
}
#[test]
fn file_comments_v75_draft_encoding_refuses_escaped_overflow_before_provider_or_json_allocation() {
    let real = real(Vec::new());
    let intent = file::FileCommentIntentV75::capture(&real.app).unwrap();
    assert!(file::FileCommentPreviewV75::prepare(
        intent,
        Arc::from("\u{1}".repeat(65_536)),
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        deadline()
    )
    .is_err());
    assert!(file::decode_draft(
        "{\"version\":75,\"values\":{\"body\":\"x\"},\"publication\":null,\"unknown\":1}"
    )
    .is_err());
    let reference = json!({
        "attempt_id":1,
        "pr_node_id":"PR_7",
        "observed_head":"a".repeat(40),
        "payload_sha256":"a".repeat(64),
    });
    let valid = json!({"version":75,"values":{"body":FILE_BODY},"publication":reference});
    let (body, actual) = file::decode_draft(&valid.to_string()).unwrap();
    assert_eq!(body, FILE_BODY);
    assert_eq!(actual.unwrap().attempt_id, 1);
    let mut extra = valid.clone();
    extra["publication"]["permission"] = json!("must never restore authority");
    assert!(file::decode_draft(&extra.to_string()).is_err());
    let mut missing = valid;
    missing["publication"]
        .as_object_mut()
        .unwrap()
        .remove("pr_node_id");
    assert!(file::decode_draft(&missing.to_string()).is_err());
    intact(&real);
}
#[test]
fn file_comments_v75_atomic_origin_failure_rolls_back_same_attempt_and_preserves_original_tables() {
    let real = real(phase_steps(members(), true));
    let actual = fresh(&real).unwrap();
    let seed = FileCommentOriginSeedV75::from_producer(
        &database(&real.fixture),
        &real.app.github.as_ref().unwrap().identity,
        &actual,
    )
    .unwrap();
    let target = PrWriteTarget::for_source(&seed.scope, REPO, &seed.pr_node, seed.number).unwrap();
    let write = seed.write(FILE_BODY).unwrap();
    let mut journal = PrWriteJournal::open(&database(&real.fixture)).unwrap();
    writable_fixture_sql(&real.fixture).execute_batch("CREATE TRIGGER reject_file_origin_v75 BEFORE INSERT ON pr_write_file_comment_origins_v75 BEGIN SELECT RAISE(ABORT,'actual local origin failure'); END;").unwrap();
    assert!(journal
        .begin_file_comment_v75(&target, &write, &seed)
        .is_err());
    assert_eq!(
        sql(&real.fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        sql(&real.fixture)
            .query_row(
                "SELECT count(*) FROM pr_write_file_comment_origins_v75",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    writable_fixture_sql(&real.fixture)
        .execute_batch("DROP TRIGGER reject_file_origin_v75")
        .unwrap();
    intact(&real);
}
#[test]
fn file_comments_v75_actual_confirm_uses_original_ordered_sql_ack_one_post_three_receipts_and_terminal_fence(
) {
    let mut steps = observation();
    steps.extend(publish_opening());
    steps.extend(last_guard());
    steps.extend(close_phase());
    steps.push(post_file(201, rest_file()));
    steps.push(get(
        &format!("repos/{REPO}/pulls/comments/9181"),
        rest_file(),
    ));
    steps.push(node_step(node_file()));
    steps.extend(file_visibility());
    let real = real(steps);
    let intent = file::FileCommentIntentV75::capture(&real.app).unwrap();
    let preview = file::FileCommentPreviewV75::prepare(
        intent.clone(),
        Arc::from(FILE_BODY),
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        deadline(),
    )
    .unwrap();
    let mut writer = DraftWriter::new(database(&real.fixture));
    writer
        .save(
            intent.scope().clone(),
            intent.draft_key().into(),
            file::encode_draft(FILE_BODY, None).unwrap(),
        )
        .unwrap();
    let client = writer.client();
    let cancel = CancellationToken::default();
    let deadline = deadline();
    let fence = client
        .capture_publication_v2(
            &database(&real.fixture),
            intent.scope(),
            intent.draft_key(),
            &cancel,
            deadline,
        )
        .unwrap();
    let result = preview
        .execute(
            &provider(&real.fixture),
            &database(&real.fixture),
            &client,
            fence,
            true,
            None,
            &cancel,
            deadline,
        )
        .unwrap();
    assert_eq!(
        result.state(),
        file::FileCommentPublicationStateV75::ConfirmedSnapshotRetired
    );
    assert_eq!(result.confirmed_comment_id(), Some(9181));
    assert!(result.source_retired());
    assert!(Cache::open(&database(&real.fixture))
        .unwrap()
        .bounded_draft(intent.scope(), intent.draft_key(), 128 * 1024)
        .unwrap()
        .is_none());
    assert_eq!(
        sql(&real.fixture)
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE command='file-comment-create-v75'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "confirmed"
    );
    assert_eq!(
        sql(&real.fixture)
            .query_row(
                "SELECT count(*) FROM pr_write_file_comment_origins_v75",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        1
    );
    exact_protocol(&real, 1);
}
#[test]
fn file_comments_v75_protected_history_reopens_exact_body_from_fresh_summary_without_current_member_or_write(
) {
    let mut steps = phase_steps(members(), true);
    steps.extend(history_phase(42));
    steps.extend(history_phase(42));
    let mut real = real(steps);
    let actual = fresh(&real).unwrap();
    let seed = FileCommentOriginSeedV75::from_producer(
        &database(&real.fixture),
        &real.app.github.as_ref().unwrap().identity,
        &actual,
    )
    .unwrap();
    let target = PrWriteTarget::for_source(&seed.scope, REPO, &seed.pr_node, seed.number).unwrap();
    let write = seed.write(FILE_BODY).unwrap();
    let mut journal = PrWriteJournal::open(&database(&real.fixture)).unwrap();
    let permit = journal
        .begin_file_comment_v75(&target, &write, &seed)
        .unwrap();
    Cache::open(&database(&real.fixture))
        .unwrap()
        .put_draft(
            &seed.scope,
            &seed.draft_key().unwrap(),
            &file::encode_draft(FILE_BODY, Some(&permit.marker().publication_reference())).unwrap(),
        )
        .unwrap();
    let expected = sql_state(&real.fixture);
    summary(&mut real);
    let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
    let listed = history::files_v75::load(
        intent.clone(),
        None,
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        deadline(),
    )
    .unwrap();
    assert_eq!(listed.labels.len(), 1);
    assert!(listed.detail.is_empty());
    let opened = history::files_v75::load(
        intent,
        Some((&listed, 0)),
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        deadline(),
    )
    .unwrap();
    assert!(opened.detail.iter().any(|line| line == FILE_BODY));
    assert!(opened
        .detail
        .iter()
        .any(|line| line.contains("no invented line 1")));
    assert_eq!(sql_state(&real.fixture), expected);
    exact_protocol(&real, 0);
}
#[test]
fn file_comments_v75_history_account_loss_refuses_before_origin_body_and_keeps_all_protected_sql() {
    let mut real = real(history_phase(43));
    summary(&mut real);
    let before = sql_state(&real.fixture);
    let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
    assert!(history::files_v75::load(
        intent,
        None,
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        deadline()
    )
    .is_err());
    assert_eq!(sql_state(&real.fixture), before);
    exact_protocol(&real, 0);
}
