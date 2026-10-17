//! Genuine Base publisher and SQLite effects. These are code contracts, not
//! keyboard/mouse evidence; the native cohort is independently declared.
use super::*;
use crate::{
    drafts::DraftWriter,
    forms::{Form, FormAction},
    pr_actions::PrPlan,
    pr_write_journal::{PrWriteJournal, PrWriteState, PrWriteTarget},
    scoped_pr_collaboration_plan_v17::ConversationPublicationStateV17 as State,
    scoped_pr_collaboration_ui_v17::{self as ui, metadata_base_v32 as base},
};
const TIP: &str = "cccccccccccccccccccccccccccccccccccccccc";
#[path = "scoped_pr_metadata_base_history_close_v296_tests.rs"]
mod close_v296;
#[path = "scoped_pr_metadata_base_feedback_v314_tests.rs"]
mod feedback_v314;

#[test]
fn pr_metadata_base_v32_genuine_editable_input_explains_typed_choice_and_preserves_preview() {
    use crate::{app::Action, theme::ColorMode};
    use crossterm::event::{KeyCode, KeyModifiers};
    const GUIDANCE: &str = "Base fields are read only; use F2 Choose branch, then Preview or Close";
    const GENERIC: &str = "This Conversation state is read only; use actual typed choices or Close";
    fn full_sql(f: &Fixture) -> Vec<String> {
        let db = rusqlite::Connection::open_with_flags(
            database(f),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let schema = db
            .prepare("SELECT type,name,COALESCE(sql,'') FROM sqlite_master ORDER BY type,name")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        let mut result = vec![format!("schema:{schema:?}")];
        for (kind, name, _) in schema {
            if kind != "table" {
                continue;
            }
            let mut st = db
                .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
                .unwrap();
            let cols = st.column_count();
            let mut rows = st
                .query_map([], |row| {
                    (0..cols)
                        .map(|i| row.get::<_, rusqlite::types::Value>(i))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| format!("{row:?}"));
            result.push(format!("{name}:{rows:?}"));
        }
        result
    }
    let mut s = setup(catalogue_phase(
        vec![json!([
            branch("main", &"b".repeat(40)),
            branch("next", TIP)
        ])],
        json!(true),
    ));
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let mut form = base::open(
        original.clone(),
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    form.dispatch(&FormAction::MetadataPicker(
        crate::pr_metadata_picker::PrMetadataPickerAction::Close,
    ));
    assert!(base::picker(&form).is_none());
    assert!(base::request(&form).is_err());
    assert_eq!(form.focus, 0);
    let intent = Arc::clone(&base::state(&form).unwrap().intent);
    let ticket = Arc::clone(&base::state(&form).unwrap().ticket);
    let values = ui::encoded_draft(&form).unwrap();
    let before_sql = full_sql(&s.fixture);
    let commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(
        ui::validate_input(&form, "typed-name").unwrap_err(),
        GUIDANCE
    );
    assert!(form.key(KeyCode::Char('x'), KeyModifiers::NONE).is_none());
    assert_eq!(form.error, GUIDANCE);
    form.paste("not a selected actual branch");
    assert_eq!(form.error, GUIDANCE);
    assert!(
        form.error.len() <= GENERIC.len(),
        "No larger failure caption"
    );
    assert_eq!(ui::encoded_draft(&form).unwrap(), values);
    assert!(Arc::ptr_eq(&base::state(&form).unwrap().intent, &intent));
    assert!(Arc::ptr_eq(&base::state(&form).unwrap().ticket, &ticket));
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(crate::app::Overlay::Form);
    for theme in 0..3 {
        s.app.settings_theme = theme;
        s.app.no_color = theme == 2;
        s.app.color_mode = if theme == 2 {
            ColorMode::NoColor
        } else {
            ColorMode::TrueColor
        };
        for (width, height) in [(80, 24), (120, 40)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut s.app))
                .unwrap();
            let form = s
                .app
                .form
                .as_ref()
                .expect("Same actual Base Form survives paint");
            assert!(!form.preview && form.publication.is_none() && !form.submitting);
            assert!(base::request(form).is_err());
            assert!(Arc::ptr_eq(&base::state(form).unwrap().intent, &intent));
            assert!(Arc::ptr_eq(&base::state(form).unwrap().ticket, &ticket));
            assert_eq!(ui::encoded_draft(form).unwrap(), values);
            assert_eq!(form.fields.len(), 2);
            assert!(form.fields.iter().all(|field| field.editor.text.is_empty()));
            assert_eq!(form.field_rects.len(), 2);
            assert!(form.field_rects.iter().all(|rect| !rect.is_empty()));
            let error = form.error_rect;
            assert!(usize::from(error.width) >= GUIDANCE.len() && error.height > 0);
            let visible = (error.x..error.x + u16::try_from(GUIDANCE.len()).unwrap())
                .map(|x| terminal.backend().buffer()[(x, error.y)].symbol())
                .collect::<String>();
            assert_eq!(
                visible, GUIDANCE,
                "Complete actual error row at {width}x{height}, theme{theme}"
            );
            let text = terminal
                .backend()
                .buffer()
                .content
                .chunks(usize::from(width))
                .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            for label in ["[F2 Choose branch]", "[Preview]", "[Esc Close]"] {
                assert!(text.contains(label));
            }
            assert!(!text.contains("[Confirm action]"));
            assert!(s
                .app
                .hits
                .iter()
                .any(|hit| matches!(&hit.action, Action::Form(FormAction::OpenPicker(0)))));
            assert!(s
                .app
                .hits
                .iter()
                .any(|hit| matches!(&hit.action, Action::Form(FormAction::Preview))));
            assert!(s
                .app
                .hits
                .iter()
                .any(|hit| matches!(&hit.action, Action::Form(FormAction::Close))));
        }
    }
    assert_eq!(full_sql(&s.fixture), before_sql);
    assert_eq!(
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        commands
    );
    calls_and_git(&s, s.steps.len());

    // Genuine prepared Preview, not a hand-built state. The validator retains
    // its old refusal; real preview paste still returns early without an error.
    let (s, mut form, preview) = prepared(vec![]);
    let before_sql = full_sql(&s.fixture);
    let values = ui::encoded_draft(&form).unwrap();
    let error = form.error.clone();
    assert!(form.preview);
    assert_eq!(ui::validate_input(&form, "ignored").unwrap_err(), GENERIC);
    form.paste("ignored actual Preview paste");
    assert_eq!(form.error, error);
    assert_eq!(ui::encoded_draft(&form).unwrap(), values);
    assert!(Arc::ptr_eq(
        base::state(&form).unwrap().preview.as_ref().unwrap(),
        &preview
    ));
    assert_eq!(full_sql(&s.fixture), before_sql);
    calls_and_git(&s, s.steps.len());
}

#[test]
fn pr_metadata_base_v32_held_paid_final_uid_preserves_exact_draft_until_success_and_loss() {
    use super::super::protocol::publication::refresh_v19::{
        held_publication_v84, protected_rows_v84,
    };
    for uid in [42, 43] {
        let mut calls = paid();
        *calls.last_mut().unwrap() = get("user", json!({"id":uid,"login":"octocat"}));
        calls.last_mut().unwrap().delay_ms = 2000;
        let (s, form, preview) = prepared(calls);
        let (outcome, (body, rows)) = held_publication_v84(
            &s,
            &form,
            false,
            |api, client, fence, cancel, deadline| {
                PrPlan::execute_original_metadata_base_v32(
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
            |_, _| {
                let body = Cache::open(&database(&s.fixture))
                    .unwrap()
                    .draft(&form.scope(), &form.resource())
                    .unwrap()
                    .unwrap();
                let envelope: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(envelope["values"], json!({"base":"next","tip":TIP}));
                assert!(envelope["publication"]["attempt_id"].as_i64().unwrap() > 0);
                (body, protected_rows_v84(&s))
            },
        );
        let after = Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&form.scope(), &form.resource())
            .unwrap();
        if uid == 42 {
            assert_eq!(outcome.state, State::Confirmed);
            assert!(!outcome.draft_retained());
            assert!(outcome.refreshed_page_v19.is_some());
            assert_eq!(after, None);
        } else {
            assert_eq!(outcome.state, State::ConfirmedVisibilityUnavailable);
            assert!(outcome.draft_retained());
            assert!(outcome.refreshed_page_v19.is_none());
            assert!(outcome.return_v19.is_none());
            assert_eq!(after.as_deref(), Some(body.as_str()));
            assert_eq!(protected_rows_v84(&s), rows);
        }
        assert_marker(&s, &form, &outcome, PrWriteState::Confirmed);
        exact_mixed(&s, s.steps.len());
    }
}

#[test]
fn pr_metadata_base_v32_held_paid_uid_foreign_ack_epoch_and_inherited_expiry_preserve_protected_rows(
) {
    use super::super::protocol::publication::refresh_v19::{
        held_publication_v84, protected_rows_v84,
    };
    for expired in [false, true] {
        let mut calls = paid();
        calls.last_mut().unwrap().delay_ms = 2000;
        let (s, form, preview) = prepared(calls);
        let (outcome, before) = held_publication_v84(
            &s,
            &form,
            expired,
            |api, client, fence, cancel, deadline| {
                PrPlan::execute_original_metadata_base_v32(
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
                    client
                        .save_and_wait(form.scope(), form.resource(), body)
                        .unwrap();
                }
                protected_rows_v84(&s)
            },
        );
        assert_eq!(outcome.state, State::ConfirmedVisibilityUnavailable);
        assert!(outcome.draft_retained());
        assert!(outcome.refreshed_page_v19.is_none());
        assert_eq!(protected_rows_v84(&s), before);
        assert_marker(&s, &form, &outcome, PrWriteState::Confirmed);
        exact_mixed(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_base_v32_draft_closed_profile_preserves_exact_version17_and_conversation_bytes() {
    use crate::scoped_pr_collaboration_draft_v17::{encode_base_draft_v32, encode_draft_v17};
    let values = BTreeMap::from([
        ("base".into(), String::new()),
        ("tip".into(), String::new()),
    ]);
    assert_eq!(
        encode_base_draft_v32(&values, None).unwrap(),
        "{\"version\":17,\"values\":{\"base\":\"\",\"tip\":\"\"},\"publication\":null}"
    );
    assert!(encode_draft_v17(&values, None).is_err());
    let body = BTreeMap::from([("body".into(), "A\nB".into())]);
    assert_eq!(
        encode_draft_v17(&body, None).unwrap(),
        "{\"version\":17,\"values\":{\"body\":\"A\\nB\"},\"publication\":null}"
    );
    assert!(encode_base_draft_v32(&body, None).is_err());
}
#[test]
fn pr_metadata_base_v32_draft_profile_counts_utf8_bytes_and_refuses_foreign_or_oversize_fields() {
    use crate::scoped_pr_collaboration_draft_v17::encode_base_draft_v32;
    let base_name = format!("{}a", "界".repeat(341));
    assert_eq!(base_name.len(), 1024);
    let values = BTreeMap::from([("base".into(), base_name), ("tip".into(), "a".repeat(64))]);
    let encoded = encode_base_draft_v32(&values, None).unwrap();
    let decoded: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded["values"], json!(values));
    assert!(encoded.len() < 128 * 1024);
    for (key, replacement) in [("base", "界".repeat(342)), ("tip", "a".repeat(65))] {
        let mut bad = values.clone();
        bad.insert(key.into(), replacement);
        assert!(encode_base_draft_v32(&bad, None).is_err());
    }
    let mut missing = values.clone();
    missing.remove("tip");
    assert!(encode_base_draft_v32(&missing, None).is_err());
    let mut foreign = values;
    foreign.insert("body".into(), "not a Base field".into());
    assert!(encode_base_draft_v32(&foreign, None).is_err());
}
#[test]
fn pr_metadata_base_v32_genuine_form_draft_profile_refuses_before_copy_or_new_effects() {
    let (s, mut form, _) = prepared(vec![]);
    let before = sql(&s.fixture);
    assert!(base::request(&form).is_ok());
    let encoded: Value = serde_json::from_str(&ui::encoded_draft(&form).unwrap()).unwrap();
    assert_eq!(encoded["values"], json!({"base":"next","tip":TIP}));
    for (index, value) in [(0, "x".repeat(1025)), (1, "a".repeat(65))] {
        let original = std::mem::replace(&mut form.fields[index].editor.text, value);
        assert!(base::bounded_values_v32(&form).is_err());
        assert!(base::request(&form).is_err());
        assert!(ui::encoded_draft(&form).is_err());
        form.fields[index].editor.text = original;
    }
    form.fields[1].key = "body".into();
    assert!(base::bounded_values_v32(&form).is_err());
    assert!(ui::encoded_draft(&form).is_err());
    assert_eq!(sql(&s.fixture), before);
    exact_mixed(&s, s.steps.len());
}
fn full_raw(changed: bool) -> Value {
    let mut raw = parent();
    raw["additions"] = json!(1);
    raw["deletions"] = json!(1);
    raw["created_at"] = json!("2026-10-16T01:00:00Z");
    raw["updated_at"] = json!("2026-10-16T02:00:00Z");
    raw["mergeable"] = Value::Null;
    raw["mergeable_state"] = json!("unknown");
    if changed {
        raw["base"]["sha"] = json!(TIP);
        raw["base"]["ref"] = json!("next");
    }
    raw
}
fn receipts() -> Vec<GhStep> {
    let endpoint = format!("repos/{REPO}/pulls/7");
    let mut patch = get(&endpoint, full_raw(true));
    *patch.args.iter_mut().find(|a| a.as_str() == "GET").unwrap() = "PATCH".into();
    patch.args.extend(["--input".into(), "-".into()]);
    patch.stdin = serde_json::to_vec(&json!({"base":"next"})).unwrap();
    vec![patch, get(&endpoint, full_raw(true))]
}
fn visibility() -> Vec<GhStep> {
    let mut calls = vec![
        get("user", json!({"id":42,"login":"octocat"})),
        get(&format!("repos/{REPO}"), repository()),
        get(&format!("repos/{REPO}/pulls/7"), full_raw(true)),
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
    calls.push(get(&format!("repos/{REPO}/pulls/7"), full_raw(true)));
    calls.push(get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(calls.len(), 10);
    calls
}
fn paid() -> Vec<GhStep> {
    let mut calls = selected_phase("next", TIP);
    calls.extend(receipts());
    calls.extend(visibility());
    assert_eq!(calls.len(), 23);
    calls
}
fn prepared(tail: Vec<GhStep>) -> (Setup, Form, Arc<base::BasePreviewV32>) {
    let mut calls = catalogue_phase(
        vec![json!([
            branch("main", &"b".repeat(40)),
            branch("next", TIP)
        ])],
        json!(true),
    );
    calls.extend(selected_phase("next", TIP));
    calls.extend(tail);
    let mut s = setup(calls);
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let mut form = base::open(
        original.clone(),
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    assert!(
        base::request(&form).is_err(),
        "No eligibility from initial/stored text"
    );
    form.dispatch(&FormAction::MetadataPicker(
        crate::pr_metadata_picker::PrMetadataPickerAction::IndexedRow(1),
    ));
    form.dispatch(&FormAction::MetadataPicker(
        crate::pr_metadata_picker::PrMetadataPickerAction::Choose,
    ));
    let request = base::request(&form).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let prepared = base::BasePreviewV32::prepare(
        &request,
        &provider,
        &database(&s.fixture),
        &cancel,
        deadline,
    )
    .unwrap();
    let preview = match prepared {
        base::BasePreviewResultV39::Prepared(preview) => preview,
        base::BasePreviewResultV39::NoChange(_) => {
            panic!("Actual requested next member must differ from main")
        }
    };
    assert!(base::accept_preview(&mut form, &request, preview.clone()));
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(crate::app::Overlay::Form);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, &mut s.app)).unwrap();
    assert!(base::request(s.app.form.as_ref().unwrap())
        .unwrap()
        .matches_confirm(s.app.form.as_ref().unwrap()));
    let form = s.app.form.take().unwrap();
    s.app.overlay = None;
    (s, form, preview)
}
fn execute(
    s: &Setup,
    form: &Form,
    preview: &Arc<base::BasePreviewV32>,
) -> GithubResult<crate::scoped_pr_collaboration_plan_v17::ConversationPublicationV17> {
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
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    PrPlan::execute_original_metadata_base_v32(
        preview,
        &provider,
        &database(&s.fixture),
        &client,
        fence,
        &cancel,
        deadline,
    )
}
fn target(form: &Form) -> PrWriteTarget {
    let s = base::state(form).unwrap();
    PrWriteTarget::new(
        s.intent.source.identity(),
        s.intent.source.parent().summary(),
    )
    .unwrap()
}
fn exact_mixed(s: &Setup, count: usize) {
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    for (i, (actual, expected)) in rows.iter().zip(&s.steps).enumerate() {
        assert_eq!(actual["step"], i);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(
            actual["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (p, b) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(p)).unwrap(), b);
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
fn assert_marker(
    s: &Setup,
    form: &Form,
    outcome: &crate::scoped_pr_collaboration_plan_v17::ConversationPublicationV17,
    state: PrWriteState,
) {
    let j = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let target = target(form);
    let marker = j
        .original_publication(&target, &outcome.reference)
        .unwrap()
        .unwrap();
    assert_eq!(marker.state, state);
    assert_eq!(marker.command, "metadata-base-v32");
    let origin = j.metadata_base_origin_v32(&target, &marker).unwrap();
    assert_eq!(origin.seed.historical_base(), "b".repeat(40));
    assert_eq!(origin.seed.requested_tip(), TIP);
    assert_eq!(origin.seed.requested_ref(), "next");
}
#[test]
fn pr_metadata_base_v32_actual_single_field_patch_get_cursor23_atomic_new_base_page_and_same_attempt_pair(
) {
    let (s, form, preview) = prepared(paid());
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::Confirmed);
    assert!(!outcome.draft_retained());
    assert!(outcome.refreshed_page_v19.is_some());
    assert_marker(&s, &form, &outcome, PrWriteState::Confirmed);
    let target = target(&form);
    let c = Cache::open(&database(&s.fixture)).unwrap();
    let record = c
        .get(target.scope(), "pr-details", target.node_id())
        .unwrap()
        .unwrap();
    assert_eq!(record.payload["summary"]["base_ref"], "next");
    assert_eq!(record.payload["summary"]["base_sha"], TIP);
    assert!(c.draft(target.scope(), &form.resource()).unwrap().is_none());
    let db = rusqlite::Connection::open(database(&s.fixture)).unwrap();
    let mut st=db.prepare("SELECT kind,mutation_ordinal FROM sync_mutation_fences WHERE host=?1 AND login=?2 AND resource=?3 ORDER BY kind").unwrap();
    let rows = st
        .query_map(
            rusqlite::params![target.scope().host, target.scope().login, target.node_id()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        vec![
            ("inline_comment".into(), outcome.reference.attempt_id),
            ("review".into(), outcome.reference.attempt_id)
        ]
    );
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_raw_patch_and_get_wrong_tip_ref_h_numeric_target_each_stays_uncertain_no_fallback(
) {
    for child in [0, 1] {
        for field in ["tip", "ref", "head", "numeric"] {
            let mut calls = selected_phase("next", TIP);
            let mut receipt = receipts();
            let mut raw = full_raw(true);
            match field {
                "tip" => raw["base"]["sha"] = json!("d".repeat(40)),
                "ref" => raw["base"]["ref"] = json!("elsewhere"),
                "head" => raw["head"]["sha"] = json!("d".repeat(40)),
                _ => raw["id"] = json!(1008),
            };
            receipt[child].stdout = serde_json::to_vec(&raw).unwrap();
            receipt.truncate(child + 1);
            calls.extend(receipt);
            let (s, form, preview) = prepared(calls);
            let outcome = execute(&s, &form, &preview).unwrap();
            assert_eq!(outcome.state, State::Uncertain);
            assert!(outcome.effect.is_none());
            assert!(outcome.draft_retained());
            assert_marker(&s, &form, &outcome, PrWriteState::Uncertain);
            exact_mixed(&s, s.steps.len());
        }
    }
}
#[test]
fn pr_metadata_base_v32_raw_errors_beside_patch_data_are_not_transition_or_current_source() {
    let mut calls = selected_phase("next", TIP);
    let mut response = full_raw(true);
    response["errors"] = json!([{"message":"actual uncertainty"}]);
    let mut patch = receipts().remove(0);
    patch.stdout = serde_json::to_vec(&response).unwrap();
    calls.push(patch);
    let (s, form, preview) = prepared(calls);
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::Uncertain);
    assert!(outcome.refreshed_page_v19.is_none());
    assert!(outcome.return_v19.is_none());
    assert_marker(&s, &form, &outcome, PrWriteState::Uncertain);
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_prepost_actual_selected_tip_drift_retains_protected_not_started_and_no_patch(
) {
    let mut calls = selected_phase("next", TIP);
    calls[5].stdout = serde_json::to_vec(&branch("next", &"d".repeat(40))).unwrap();
    calls.truncate(6);
    let (s, form, preview) = prepared(calls);
    assert!(execute(&s, &form, &preview).is_err());
    let db = rusqlite::Connection::open(database(&s.fixture)).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_final_prepost_actual_uid_change_protects_claim_and_does_not_invoke_patch() {
    let mut calls = selected_phase("next", TIP);
    calls.last_mut().unwrap().stdout =
        serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    let (s, form, preview) = prepared(calls);
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::NotStartedProtected);
    assert!(outcome.draft_retained());
    assert_marker(&s, &form, &outcome, PrWriteState::Pending);
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_final_visibility_uid_loss_keeps_confirmed_origin_and_no_private_new_base_page(
) {
    let mut calls = paid();
    calls.last_mut().unwrap().stdout =
        serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    let (s, form, preview) = prepared(calls);
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::ConfirmedVisibilityUnavailable);
    assert!(outcome.refreshed_page_v19.is_none());
    assert!(outcome.return_v19.is_none());
    assert_marker(&s, &form, &outcome, PrWriteState::Confirmed);
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_real_cleanup_sql_failure_retains_confirmed_origin_exact_reference_and_both_fences(
) {
    let (s, form, preview) = prepared(paid());
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER fail_base_cleanup_v32 BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'metadata-base-v32/%' BEGIN SELECT RAISE(ABORT,'actual Base cleanup refuses');END;").unwrap();
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::ConfirmedVisibilityUnavailable);
    assert!(outcome.draft_retained());
    assert!(outcome.return_v19.is_some());
    assert_marker(&s, &form, &outcome, PrWriteState::Confirmed);
    let saved = Cache::open(&database(&s.fixture))
        .unwrap()
        .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
        .unwrap()
        .unwrap();
    let envelope: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(envelope["values"], json!({"base":"next","tip":TIP}));
    assert_eq!(
        envelope["publication"],
        serde_json::to_value(&outcome.reference).unwrap()
    );
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_actual_confirmed_cleanup_failure_then_personal_refresh_consumes_exact_seven_read_children(
) {
    use crate::{
        app::{Action, Overlay, View},
        runtime::Background,
        scoped_pr_collaboration_native_v17 as native,
    };
    // Cleanup is attempted only AFTER the complete paid final numeric UID.
    let mut tail = paid();
    let mut fresh = principal();
    fresh.push(get(&format!("repos/{REPO}"), repository()));
    fresh.extend(visibility()[2..9].iter().cloned());
    fresh.push(get(&format!("repos/{REPO}"), repository()));
    fresh.extend(principal());
    assert_eq!(
        fresh.len(),
        13,
        "Same six Core and exactly seven measured children"
    );
    tail.extend(fresh);
    let (mut s, form, _) = prepared(tail);
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER retain_base_refresh_v180 BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'metadata-base-v32/%' BEGIN SELECT RAISE(ABORT,'actual Base cleanup failure'); END;").unwrap();
    let mut writer = DraftWriter::new(database(&s.fixture));
    writer
        .save(
            form.scope(),
            form.resource(),
            ui::encoded_draft(&form).unwrap(),
        )
        .unwrap();
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(Overlay::Form);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    for _ in 0..2 {
        terminal.draw(|f| crate::ui::draw(f, &mut s.app)).unwrap();
    }
    s.app.dispatch(Action::Form(FormAction::Submit));
    native::prepare_requested(&mut s.app);
    let work = s
        .app
        .scoped_pr_collaboration_work_v17
        .take()
        .expect("Genuine exact Base Confirm work");
    assert_eq!(work.phase(), native::ConversationPhaseKindV17::Confirm);
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client(),
    ));
    let settle = |app: &mut crate::app::App, background: &mut Background| {
        let until = Instant::now() + Duration::from_secs(8);
        while app.loading[View::Inbox.index()] {
            background.poll(app);
            assert!(Instant::now() < until, "Actual Base owner must settle");
            std::thread::yield_now();
        }
    };
    settle(&mut s.app, &mut background);
    assert!(s.app.github.is_none());
    assert!(s.app.scoped_pr_collaboration_return_v19.is_some());
    let protected = || {
        [
            "SELECT json_array(attempt_id,command,state,confirmed_json,created_at) FROM pr_write_attempts ORDER BY attempt_id",
            "SELECT json_array(attempt_id,repository_id,pr_id,origin_json) FROM pr_write_metadata_base_origins_v32 ORDER BY attempt_id",
            "SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource",
            "SELECT json_array(kind,mutation_ordinal) FROM sync_mutation_fences ORDER BY kind",
        ].into_iter().flat_map(|q|db.prepare(q).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect::<Vec<_>>()
    };
    let original = protected();
    assert_eq!(
        db.query_row("SELECT state FROM pr_write_attempts", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "confirmed"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sync_mutation_fences WHERE kind IN('review','inline_comment')",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        2
    );
    terminal.draw(|f| crate::ui::draw(f, &mut s.app)).unwrap();
    s.app
        .dispatch(Action::Form(FormAction::RefreshConversationV19));
    native::prepare_requested(&mut s.app);
    let work = s
        .app
        .scoped_pr_collaboration_work_v17
        .take()
        .expect("Genuine NEW original read, not paid clock renewal");
    assert_eq!(
        work.phase(),
        native::ConversationPhaseKindV17::RefreshExactV19
    );
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client(),
    ));
    settle(&mut s.app, &mut background);
    let page = s.app.github.as_ref().and_then(|w| w.pr.as_ref()).expect(
        "Exact seventh closing raw PR and final Core closures must admit actual fresh B1 page",
    );
    assert_eq!(page.details.summary.base_sha, TIP);
    assert_eq!(page.details.summary.base_ref, "next");
    assert!(s.app.scoped_pr_collaboration_return_v19.is_none());
    assert_eq!(
        protected(),
        original,
        "Read cannot rewrite/delete the original body, receipt, origin or once-fence pair"
    );
    exact_mixed(&s, s.steps.len());
    writer.poll();
}

#[test]
fn pr_metadata_base_v32_preview_once_cannot_resend_same_protected_reference_or_renew_approval() {
    let mut tail = selected_phase("next", TIP);
    let mut raw = full_raw(true);
    raw["base"]["sha"] = json!("d".repeat(40));
    let mut patch = receipts().remove(0);
    patch.stdout = serde_json::to_vec(&raw).unwrap();
    tail.push(patch);
    let (s, form, preview) = prepared(tail);
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::Uncertain);
    let before = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let saved = Cache::open(&database(&s.fixture))
        .unwrap()
        .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
        .unwrap();
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
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    assert!(PrPlan::execute_original_metadata_base_v32(
        &preview,
        &provider,
        &database(&s.fixture),
        &client,
        fence,
        &cancel,
        deadline
    )
    .is_err());
    assert_eq!(
        saved,
        Cache::open(&database(&s.fixture))
            .unwrap()
            .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
            .unwrap()
    );
    assert_eq!(
        before,
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap()
    );
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_actual_form_resize_and_rechoose_revoke_exact_painted_selection_without_io()
{
    let (mut s, form, preview) = prepared(Vec::new());
    s.app.form = Some(form);
    s.app.overlay = Some(crate::app::Overlay::Form);
    let request = base::request(s.app.form.as_ref().unwrap()).unwrap();
    assert!(request.matches_confirm(s.app.form.as_ref().unwrap()));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 18)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, &mut s.app)).unwrap();
    assert!(!request.matches_confirm(s.app.form.as_ref().unwrap()));
    s.app.form.as_mut().unwrap().dispatch(&FormAction::Close);
    assert!(!request.matches_confirm(s.app.form.as_ref().unwrap()));
    assert!(!preview.used.load(std::sync::atomic::Ordering::Acquire));
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_real_menu_uses_only_closed_base_and_reachable_base_history_actions() {
    let s = setup(Vec::new());
    let controls = s.app.pr_controls();
    assert!(controls
        .iter()
        .any(|(_, a)| *a == crate::app::Action::Pr(crate::app::PrUiAction::MetadataBaseV32)));
    assert!(
        controls
            .iter()
            .any(|(_, a)| *a
                == crate::app::Action::Pr(crate::app::PrUiAction::MetadataBaseHistoryV32))
    );
    assert!(!controls.iter().any(|(_, a)| *a
        == crate::app::Action::Github(crate::github_workspace::GithubAction::Compose(
            crate::forms::PrFormKind::Base
        ))));
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_actual_base_origin_grafted_opposite_metadata_record_is_refused_before_history_admission(
) {
    let (s, form, preview) = prepared(paid());
    let outcome = execute(&s, &form, &preview).unwrap();
    let target = target(&form);
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let marker = journal
        .original_publication(&target, &outcome.reference)
        .unwrap()
        .unwrap();
    assert!(journal.metadata_base_origin_v32(&target, &marker).is_ok());
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute("INSERT INTO pr_write_metadata_origins_v25(attempt_id,repository_id,pr_id,origin_json)VALUES(?1,10000,1007,'{}')",[marker.attempt_id]).unwrap();
    assert!(journal.metadata_base_origin_v32(&target, &marker).is_err());
    assert!(journal
        .base_history_v32(&target, base::state(&form).unwrap().intent.source.parent())
        .is_err());
    exact_mixed(&s, s.steps.len());
}
fn history_read() -> Vec<GhStep> {
    let mut calls = parent_phase();
    calls.insert(4, capability(json!(true)));
    assert_eq!(calls.len(), 9);
    calls
}
fn unknown_with_inspection() -> (
    Setup,
    Form,
    Arc<base::BasePreviewV32>,
    crate::scoped_pr_collaboration_plan_v17::ConversationPublicationV17,
) {
    let mut tail = selected_phase("next", TIP);
    let mut patch = receipts().remove(0);
    patch.stdout =
        serde_json::to_vec(&json!({"message":"actual Base response unresolved"})).unwrap();
    tail.push(patch);
    tail.extend(history_read());
    tail.extend(history_read());
    let (s, form, preview) = prepared(tail);
    let outcome = execute(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::Uncertain);
    (s, form, preview, outcome)
}
fn inspect_history(s: &mut Setup) -> Form {
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let listed = base::history::open(
        original.clone(),
        &provider,
        &database(&s.fixture),
        &cancel,
        deadline,
        None,
    )
    .unwrap();
    assert!(base::history::state(&listed)
        .unwrap()
        .view
        .current
        .is_none());
    assert!(!base::history::ack_ready(&listed));
    let request = base::history::request(&listed).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let inspected = base::history::open(
        original,
        &provider,
        &database(&s.fixture),
        &cancel,
        deadline,
        Some(&request),
    )
    .unwrap();
    assert!(
        !base::history::ack_ready(&inspected),
        "First unpainted observation cannot ACK"
    );
    s.app.form = Some(inspected);
    s.app.overlay = Some(crate::app::Overlay::Form);
    s.app.form_dismissed = false;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, &mut s.app)).unwrap();
    assert!(base::history::ack_ready(s.app.form.as_ref().unwrap()));
    let form = s.app.form.take().unwrap();
    s.app.overlay = None;
    form
}
#[test]
fn pr_metadata_base_v32_actual_unknown_current_history_has_no_source_or_selection_and_exact_once_local_ack(
) {
    let (mut s, original, _preview, outcome) = unknown_with_inspection();
    let target = target(&original);
    let db = rusqlite::Connection::open(database(&s.fixture)).unwrap();
    let before: String = db
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'metadata-base-v32/%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let marker:String=db.query_row("SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,confirmed_json,created_at) FROM pr_write_attempts",[],|r|r.get(0)).unwrap();
    let inspected = inspect_history(&mut s);
    assert!(base::state(&inspected).is_none());
    assert!(base::request(&inspected).is_err());
    assert!(ui::encoded_draft(&inspected).is_err());
    let request = base::history::request(&inspected).unwrap();
    let current = request.view.current.as_ref().unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(current.deadline, 1024 * 1024)
        .unwrap();
    let (ack, changed) = base::history::acknowledge(
        &request,
        &provider,
        &database(&s.fixture),
        &current.cancel,
        current.deadline,
    )
    .unwrap();
    assert_eq!(changed, 1);
    assert_eq!(ack, outcome.reference);
    assert!(base::history::acknowledge(
        &request,
        &provider,
        &database(&s.fixture),
        &current.cancel,
        current.deadline
    )
    .is_err());
    assert_eq!(
        db.query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'metadata-base-v32/%'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        before
    );
    assert_eq!(db.query_row("SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,confirmed_json,created_at) FROM pr_write_attempts",[],|r|r.get::<_,String>(0)).unwrap(),marker);
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    assert!(journal.blocking(&target).unwrap().is_none());
    assert_eq!(
        journal
            .original_publication(&target, &outcome.reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Acknowledged
    );
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_actual_inspection_wrong_token_or_expired_caller_cannot_renew_ack_or_release_raw_blocker(
) {
    let (mut s, form, _preview, outcome) = unknown_with_inspection();
    let inspected = inspect_history(&mut s);
    let request = base::history::request(&inspected).unwrap();
    let current = request.view.current.as_ref().unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(current.deadline, 1024 * 1024)
        .unwrap();
    assert!(base::history::acknowledge(
        &request,
        &provider,
        &database(&s.fixture),
        &CancellationToken::default(),
        current.deadline
    )
    .is_err());
    assert!(base::history::acknowledge(
        &request,
        &provider,
        &database(&s.fixture),
        &current.cancel,
        Instant::now() - Duration::from_millis(1)
    )
    .is_err());
    assert_marker(&s, &form, &outcome, PrWriteState::Uncertain);
    assert!(
        current.ready(),
        "Invalid caller does not manufacture or consume actual inspected phase"
    );
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_actual_inspection_foreign_runner_environment_refuses_without_releasing_barrier_or_cas(
) {
    let (mut s, form, _preview, outcome) = unknown_with_inspection();
    let inspected = inspect_history(&mut s);
    let request = base::history::request(&inspected).unwrap();
    let current = request.view.current.as_ref().unwrap();
    let provider = s
        .api
        .clone()
        .with_env("BASE_FOREIGN_ISSUER", "different actual environment")
        .with_original_pr_read_bounds_v13(current.deadline, 1024 * 1024)
        .unwrap();
    assert!(base::history::acknowledge(
        &request,
        &provider,
        &database(&s.fixture),
        &current.cancel,
        current.deadline
    )
    .is_err());
    assert_marker(&s, &form, &outcome, PrWriteState::Uncertain);
    assert!(
        !current.ready(),
        "One inspected once-only observation was consumed; obtain a fresh inspection"
    );
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_actual_raw_null_text_mutation_after_inspection_refuses_cas_without_resend()
{
    let (mut s, _form, _preview, _outcome) = unknown_with_inspection();
    let inspected = inspect_history(&mut s);
    let request = base::history::request(&inspected).unwrap();
    let current = request.view.current.as_ref().unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(current.deadline, 1024 * 1024)
        .unwrap();
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute(
        "UPDATE pr_write_attempts SET confirmed_json='null' WHERE state='uncertain'",
        [],
    )
    .unwrap();
    let changed: String = db
        .query_row(
            "SELECT json_array(state,typeof(confirmed_json),confirmed_json) FROM pr_write_attempts",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(changed, "[\"uncertain\",\"text\",\"null\"]");
    assert!(base::history::acknowledge(
        &request,
        &provider,
        &database(&s.fixture),
        &current.cancel,
        current.deadline
    )
    .is_err());
    assert_eq!(
        db.query_row(
            "SELECT json_array(state,typeof(confirmed_json),confirmed_json) FROM pr_write_attempts",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        changed
    );
    assert!(!current.ready());
    exact_mixed(&s, s.steps.len());
}
fn interrupted_actual_confirmed() -> (Setup, Form, crate::pr_write_journal::PrPublicationReference)
{
    let mut tail = selected_phase("next", TIP);
    tail.extend(receipts());
    let (s, form, preview) = prepared(tail);
    let database_path = database(&s.fixture);
    base::publication::observe_next_actual_base_confirmed_v32(move |attempt| {
        assert!(attempt > 0);
        let db = rusqlite::Connection::open(&database_path).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [attempt],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "confirmed"
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1",
                [attempt],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM sync_mutation_fences", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            0,
            "Observer is BEFORE terminal fence/visibility"
        );
        panic!("Deterministic code-boundary interruption after actual confirmed Base; NOT process crash proof");
    });
    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute(&s, &form, &preview)
    }));
    assert!(interrupted.is_err());
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let target = target(&form);
    let marker = journal
        .latest_source_receipt(&target, crate::storage::SyncBindingKind::Review)
        .unwrap()
        .unwrap();
    assert_eq!(marker.state, PrWriteState::Confirmed);
    assert!(journal.metadata_base_origin_v32(&target, &marker).is_ok());
    assert!(journal.blocking(&target).unwrap().is_none());
    exact_mixed(&s, s.steps.len());
    (s, form, marker.publication_reference())
}
fn discussion(
    form: &Form,
    kind: crate::search::model::DiscussionKind,
    uid: u64,
) -> crate::search::collaboration::DiscussionTarget {
    let source = &base::state(form).unwrap().intent.source;
    let parent = source.parent();
    crate::search::collaboration::DiscussionTarget {
        scope: Scope::github("github.invalid", Some(uid)).unwrap(),
        repository_id: parent.repository_id(),
        repository: parent.repository().into(),
        parent_kind: crate::search::collaboration::ParentKind::PullRequest,
        parent_node: parent.node_id().into(),
        number: parent.number(),
        category: kind,
    }
}
fn hydrate(s: &Setup, target: &crate::search::collaboration::DiscussionTarget) -> u64 {
    let mut cache = Cache::open(&database(&s.fixture)).unwrap();
    let name = target.source();
    let generation = cache
        .reserve_resource_sync(&target.scope, &name, &target.binding().unwrap())
        .unwrap();
    assert!(cache
        .commit_reserved_sync(
            &target.scope,
            &name,
            generation,
            &[crate::storage::Record::new(
                "collaboration-document-v1",
                &name,
                &target.repository,
                json!({"body":"EXACT_OLD_BASE_PRIVATE_BODY"})
            )],
            true
        )
        .unwrap());
    generation
}
fn visible(s: &Setup, target: &crate::search::collaboration::DiscussionTarget) -> Option<u64> {
    Cache::open(&database(&s.fixture))
        .unwrap()
        .binding_status(&target.scope, &target.source())
        .unwrap()
        .unwrap()
        .visible_generation
}
fn later_single(s: &Setup, target: &PrWriteTarget, inline: bool) -> i64 {
    let mut j = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let write = crate::pr_write_journal::PrWriteIntent {
        command: if inline {
            "inline-comment"
        } else {
            "review-update-pending-v2"
        }
        .into(),
        selected_resource: if inline {
            format!("inline/{}", "d".repeat(64))
        } else {
            "review/810".into()
        },
        observed_head: "a".repeat(40),
        values: BTreeMap::from([("body".into(), "ACTUAL_LATER_LEGACY_SINGLE_INTENT".into())]),
    };
    let permit = j.begin(target, &write).unwrap();
    j.claim(&permit, target, &write).unwrap();
    j.mark_confirmed(
        &permit,
        &crate::pr_write_journal::PrConfirmedEffect {
            kind: if inline {
                crate::pr_write_journal::PrEffectKind::Comment
            } else {
                crate::pr_write_journal::PrEffectKind::Review
            },
            resource_id: if inline { "901" } else { "810" }.into(),
            node_id: Some(if inline { "NODE_901" } else { "NODE_810" }.into()),
        },
    )
    .unwrap();
    permit.marker().attempt_id
}
fn base_both_then_single(inline_later: bool) {
    use crate::search::model::DiscussionKind as Kind;
    let (s, form, base_reference) = interrupted_actual_confirmed();
    let review = discussion(&form, Kind::Review, 42);
    let inline = discussion(&form, Kind::InlineComment, 42);
    let neighbour = discussion(&form, Kind::ConversationComment, 42);
    let foreign = discussion(&form, Kind::Review, 43);
    hydrate(&s, &review);
    hydrate(&s, &inline);
    let neighbour_gen = hydrate(&s, &neighbour);
    let foreign_gen = hydrate(&s, &foreign);
    let next = later_single(&s, &target(&form), inline_later);
    assert!(next > base_reference.attempt_id);
    for order in [
        vec![review.clone(), inline.clone()],
        vec![inline.clone(), review.clone()],
    ] {
        let guard = crate::pr_projection::guard_discussion_reads(
            &database(&s.fixture),
            &review.scope,
            &order,
        )
        .unwrap();
        assert!(guard.blocked_sources().is_empty());
        assert!(guard
            .status()
            .iter()
            .any(|line| line.contains("historical composite")
                && line.contains("newer single-category")));
        crate::pr_projection::validate_discussion_read_guard(&database(&s.fixture), &guard)
            .unwrap();
        assert_eq!(visible(&s, &review), None);
        assert_eq!(visible(&s, &inline), None);
        assert_eq!(visible(&s, &neighbour), Some(neighbour_gen));
        assert_eq!(visible(&s, &foreign), Some(foreign_gen));
        let db = rusqlite::Connection::open(database(&s.fixture)).unwrap();
        let mut stmt=db.prepare("SELECT kind,mutation_ordinal FROM sync_mutation_fences WHERE host='github.invalid' AND login='user-id:42' AND resource='PERSONAL_PR_1007' ORDER BY kind").unwrap();
        let actual = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                (
                    "inline_comment".into(),
                    if inline_later {
                        next
                    } else {
                        base_reference.attempt_id
                    }
                ),
                (
                    "review".into(),
                    if inline_later {
                        base_reference.attempt_id
                    } else {
                        next
                    }
                )
            ]
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM pr_write_attempts WHERE state='confirmed'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
            2
        );
    }
    exact_mixed(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_actual_confirmed_boundary_gap_then_later_review_single_repairs_two_selected_once(
) {
    base_both_then_single(false)
}
#[test]
fn pr_metadata_base_v32_actual_confirmed_boundary_gap_then_later_inline_single_repairs_two_selected_once(
) {
    base_both_then_single(true)
}
#[test]
fn pr_metadata_base_v32_newer_malformed_known_base_origin_never_revives_older_valid_confirmed_receipt(
) {
    use crate::search::model::DiscussionKind as Kind;
    let (s, form, older) = interrupted_actual_confirmed();
    let review = discussion(&form, Kind::Review, 42);
    hydrate(&s, &review);
    let target = target(&form);
    let mut j = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let write = crate::pr_write_journal::PrWriteIntent {
        command: "metadata-base-v32".into(),
        selected_resource: "metadata-base-v32/malformed-known".into(),
        observed_head: "a".repeat(40),
        values: BTreeMap::new(),
    };
    let permit = j.begin(&target, &write).unwrap();
    j.claim(&permit, &target, &write).unwrap();
    j.mark_confirmed(
        &permit,
        &crate::pr_write_journal::PrConfirmedEffect {
            kind: crate::pr_write_journal::PrEffectKind::Metadata,
            resource_id: format!("metadata-base-v32/{}", "f".repeat(64)),
            node_id: Some(target.node_id().into()),
        },
    )
    .unwrap();
    assert!(permit.marker().attempt_id > older.attempt_id);
    assert_eq!(
        j.latest_source_receipt(&target, crate::storage::SyncBindingKind::Review)
            .unwrap()
            .unwrap()
            .attempt_id,
        permit.marker().attempt_id
    );
    let guard = crate::pr_projection::guard_discussion_reads(
        &database(&s.fixture),
        &review.scope,
        std::slice::from_ref(&review),
    );
    // A malformed newest known purpose refuses the entire private read. It
    // must not become an empty successful guard or revive the older receipt.
    let refusal = match guard {
        Err(error) => error,
        Ok(_) => panic!("Malformed newest Base original must refuse private reads"),
    };
    assert_eq!(
        refusal,
        "Known Base publication lacks its exact original observation; private text remains hidden"
    );
    assert_eq!(
        j.latest_source_receipt(&target, crate::storage::SyncBindingKind::Review)
            .unwrap()
            .unwrap()
            .attempt_id,
        permit.marker().attempt_id
    );
    assert_eq!(
        rusqlite::Connection::open(database(&s.fixture))
            .unwrap()
            .query_row("SELECT count(*) FROM sync_mutation_fences", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    exact_mixed(&s, s.steps.len());
}

#[test]
fn pr_metadata_base_v32_actual_prepared_nochange_is_distinct_from_permission_and_original_source_refusal(
) {
    for mode in ["same", "permission", "source"] {
        let mut calls = catalogue_phase(
            vec![json!([
                branch("main", &"b".repeat(40)),
                branch("next", TIP)
            ])],
            json!(true),
        );
        let mut next = if mode == "same" {
            selected_phase("main", &"b".repeat(40))
        } else {
            selected_phase("next", TIP)
        };
        next.truncate(if mode == "source" { 4 } else { 5 });
        if mode == "permission" {
            next[4] = capability(json!(false));
        }
        if mode == "source" {
            let mut raw = parent();
            raw["head"]["sha"] = json!("d".repeat(40));
            next[3].stdout = serde_json::to_vec(&raw).unwrap();
        }
        calls.extend(next);
        let s = setup(calls);
        let original =
            PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
        let provider = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
            .unwrap();
        let mut form = base::open(
            original.clone(),
            &provider,
            &database(&s.fixture),
            original.cancellation(),
            original.deadline(),
        )
        .unwrap();
        form.dispatch(&FormAction::MetadataPicker(
            crate::pr_metadata_picker::PrMetadataPickerAction::IndexedRow(usize::from(
                mode != "same",
            )),
        ));
        form.dispatch(&FormAction::MetadataPicker(
            crate::pr_metadata_picker::PrMetadataPickerAction::Choose,
        ));
        let request = base::request(&form).unwrap();
        let cancel = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        let provider = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
            .unwrap();
        match (
            mode,
            base::BasePreviewV32::prepare(
                &request,
                &provider,
                &database(&s.fixture),
                &cancel,
                deadline,
            ),
        ) {
            ("same", Ok(base::BasePreviewResultV39::NoChange(proof))) => {
                assert!(proof.matches(
                    &request.intent.source,
                    &request.selection,
                    &cancel,
                    deadline
                ));
                assert!(!proof.matches(
                    &request.intent.source,
                    &request.selection,
                    &CancellationToken::default(),
                    deadline
                ));
                assert!(!form.preview);
                assert!(base::state(&form).unwrap().preview.is_none());
                assert!(request.matches(&form));
                assert!(!request.matches_confirm(&form));
            }
            ("permission", Err(error)) => assert!(matches!(
                error.kind,
                crate::providers::gh::GithubErrorKind::Permission
            )),
            ("source", Err(error)) => {
                assert!(matches!(
                    error.kind,
                    crate::providers::gh::GithubErrorKind::StaleHead
                ))
            }
            _ => {
                panic!("Only actual prepared raw/cap NoChange may issue the closed refusal result")
            }
        }
        assert_eq!(
            rusqlite::Connection::open(database(&s.fixture))
                .unwrap()
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
        exact_mixed(&s, s.steps.len());
    }
}
