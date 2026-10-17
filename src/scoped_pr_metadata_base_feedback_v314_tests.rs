//! Real Base factory/Background and genuine writer ACK ordering contracts.
//! Main's actual ACK-to-status behavior is independently exercised by PTY.
use super::*;
use crate::{
    app::{Action, Overlay, View},
    runtime::Background,
    scoped_pr_collaboration_native_v17 as native,
};
use std::collections::BTreeMap;
const REFUSAL: &str = "Original Conversation unavailable • nothing resent • draft/journal retained";

fn full_sql(f: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        database(f),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let schema = db
        .prepare("SELECT type,name,COALESCE(sql,'') FROM sqlite_master ORDER BY type,name")
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut out = vec![format!("schema:{schema:?}")];
    for (kind, name, _) in schema {
        if kind != "table" {
            continue;
        }
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |r| {
                (0..columns)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort_by_key(|r| format!("{r:?}"));
        out.push(format!("{name}:{rows:?}"));
    }
    out
}
fn tip_failure() -> Vec<GhStep> {
    let mut phase = selected_phase("next", TIP);
    phase[5].stdout = serde_json::to_vec(&branch("next", &"d".repeat(40))).unwrap();
    phase.truncate(6);
    phase
}
fn selected_form(s: &Setup) -> Form {
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
        crate::pr_metadata_picker::PrMetadataPickerAction::IndexedRow(1),
    ));
    form.dispatch(&FormAction::MetadataPicker(
        crate::pr_metadata_picker::PrMetadataPickerAction::Choose,
    ));
    assert!(base::request(&form).is_ok());
    assert_eq!(
        form.values(),
        BTreeMap::from([("base".into(), "next".into()), ("tip".into(), TIP.into())])
    );
    form
}
fn start(
    s: &mut Setup,
    writer: &DraftWriter,
    action: FormAction,
) -> (Background, Arc<native::ScopedPrCollaborationOwnerV17>) {
    s.app.dispatch(Action::Form(action));
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(work.matches(&s.app));
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    let owner = s.app.scoped_pr_collaboration_owner_v17.clone().unwrap();
    (background, owner)
}
fn settle(s: &mut Setup, background: &mut Background) {
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
        background.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Actual original Base owner did not settle"
        );
        std::thread::yield_now();
    }
}
fn save_selected(s: &mut Setup, writer: &mut DraftWriter) -> (String, String) {
    let form = s.app.form.as_mut().unwrap();
    form.prepare_draft_autosave_v77();
    let key = form.resource();
    let body = ui::encoded_draft(form).unwrap();
    let expected: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        expected,
        json!({"version":17,"values":{"base":"next","tip":TIP},"publication":null})
    );
    writer
        .save(form.scope(), key.clone(), body.clone())
        .unwrap();
    form.settle_draft_autosave_v77(true);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        if Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&form.scope(), &key)
            .unwrap()
            .as_deref()
            == Some(body.as_str())
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "Genuine ordinary selected draft never committed"
        );
        std::thread::yield_now();
    }
    (key, body)
}
fn ack(writer: &mut DraftWriter, form: &Form, key: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let acks = writer.poll();
        if !acks.is_empty() {
            assert_eq!(acks.len(), 1);
            let actual = acks.into_iter().next().unwrap();
            assert_eq!(actual.resource, key);
            assert_eq!(actual.scope, form.scope());
            actual.result.unwrap();
            return;
        }
        assert!(
            Instant::now() < until,
            "Actual selected ordinary Save ACK missing"
        );
        std::thread::yield_now();
    }
}
fn paint_error(s: &mut Setup) {
    for (cols, rows) in [(80, 24), (120, 40)] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(cols, rows)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut s.app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let form = s.app.form.as_ref().unwrap();
        let rect = form.error_rect;
        let field_controls = s
            .app
            .hits
            .iter()
            .filter(|hit| matches!(hit.action, Action::Form(FormAction::FieldScroll(_))))
            .collect::<Vec<_>>();
        assert_eq!(field_controls.len(), 2);
        for (action, label) in [(-1, "[F5 Fields↑]"), (1, "[F6 Fields↓]")] {
            let hit = field_controls
                .iter()
                .find(|hit| hit.action == Action::Form(FormAction::FieldScroll(action)))
                .unwrap();
            let caption = (hit.rect.x..hit.rect.right())
                .map(|x| buffer[(x, hit.rect.y)].symbol())
                .collect::<String>();
            assert_eq!(caption.trim(), label);
        }
        let text = (rect.y..rect.bottom())
            .map(|y| {
                (rect.x..rect.right())
                    .map(|x| {
                        // The form's footer rectangle also contains separately
                        // verified field-scroll buttons. They are not error
                        // prose; any overwritten/missing refusal still fails
                        // the complete literal comparison below.
                        if field_controls.iter().any(|hit| {
                            x >= hit.rect.x
                                && x < hit.rect.right()
                                && y >= hit.rect.y
                                && y < hit.rect.bottom()
                        }) {
                            " "
                        } else {
                            buffer[(x, y)].symbol()
                        }
                    })
                    .collect::<String>()
                    .trim()
                    .to_owned()
            })
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(text, REFUSAL, "Full owned refusal at{cols}x{rows}");
        assert!(
            !form.preview && !form.submitting && !form.preview_requested && !form.submit_requested
        );
        assert!(base::request(form).unwrap().preview.is_none());
        assert!(s
            .app
            .hits
            .iter()
            .all(|h| h.action != Action::Form(FormAction::Submit)));
    }
}
#[test]
fn pr_metadata_base_v32_actual_refusal_retains_error_across_matching_saved_ack_before_and_after() {
    for before in [true, false] {
        let mut calls = catalogue_phase(
            vec![json!([
                branch("main", &"b".repeat(40)),
                branch("next", TIP)
            ])],
            json!(true),
        );
        calls.extend(tip_failure());
        let mut s = setup(calls);
        let form = selected_form(&s);
        s.app.form = Some(form);
        s.app.overlay = Some(Overlay::Form);
        s.app.form_dismissed = false;
        let mut writer = DraftWriter::new(database(&s.fixture));
        let (key, body) = save_selected(&mut s, &mut writer);
        if before {
            ack(&mut writer, s.app.form.as_ref().unwrap(), &key);
        }
        let sql = full_sql(&s.fixture);
        let (mut background, _) = start(&mut s, &writer, FormAction::Preview);
        settle(&mut s, &mut background);
        assert_eq!(s.app.form.as_ref().unwrap().error, REFUSAL);
        assert_eq!(s.app.form.as_ref().unwrap().error_scroll, 0);
        if !before {
            ack(&mut writer, s.app.form.as_ref().unwrap(), &key);
        }
        // Genuine ACK delivery itself has no authority to clear the Form.
        // The real Main status-overwrite ordering is the paired PTY contract.
        assert_eq!(s.app.form.as_ref().unwrap().error, REFUSAL);
        assert_eq!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .draft(&s.app.form.as_ref().unwrap().scope(), &key)
                .unwrap(),
            Some(body)
        );
        paint_error(&mut s);
        assert_eq!(full_sql(&s.fixture), sql);
        calls_and_git(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_base_v32_actual_retained_failure_never_restores_newer_account_or_confirm_form() {
    for kind in ["newer", "account", "confirm", "failed"] {
        let (mut s, form) = if kind == "confirm" {
            let (s, form, _) = prepared(tip_failure());
            (s, form)
        } else {
            let mut calls = catalogue_phase(
                vec![json!([
                    branch("main", &"b".repeat(40)),
                    branch("next", TIP)
                ])],
                json!(true),
            );
            if kind == "account" {
                let mut phase = selected_phase("next", TIP);
                phase.last_mut().unwrap().stdout =
                    serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
                calls.extend(phase);
            } else {
                calls.extend(tip_failure());
            }
            if kind == "newer" {
                calls.extend(catalogue_phase(
                    vec![json!([
                        branch("main", &"b".repeat(40)),
                        branch("next", TIP)
                    ])],
                    json!(true),
                ));
            }
            let s = setup(calls);
            let form = selected_form(&s);
            (s, form)
        };
        let opening = base::state(&form).unwrap().intent.source.opening().clone();
        s.app.form = Some(form);
        s.app.overlay = Some(Overlay::Form);
        s.app.form_dismissed = false;
        let mut writer = DraftWriter::new(database(&s.fixture));
        // A useful Confirm must keep its exact approved input; ordinary save
        // uses its real existing encoded draft without invalidating it here.
        if kind == "confirm" {
            let form = s.app.form.as_ref().unwrap();
            writer
                .save(
                    form.scope(),
                    form.resource(),
                    ui::encoded_draft(form).unwrap(),
                )
                .unwrap();
            ack(&mut writer, form, &form.resource());
        } else {
            save_selected(&mut s, &mut writer);
        }
        let sql = full_sql(&s.fixture);
        let action = if kind == "confirm" {
            FormAction::Submit
        } else {
            FormAction::Preview
        };
        let (mut background, owner) = start(&mut s, &writer, action);
        if kind == "newer" {
            // setup includes the genuine Personal startup before these fixed
            // ten catalogue + six refused selected-phase roles.
            let expected_commands = crate::personal_pr_entry_v13::tests::opening().len() + 10 + 6;
            let until = Instant::now() + Duration::from_secs(8);
            loop {
                let bytes = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
                let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |n| n + 1);
                let count = bytes[..end]
                    .split(|b| *b == b'\n')
                    .filter(|v| !v.is_empty())
                    .inspect(|v| {
                        serde_json::from_slice::<Value>(v).unwrap();
                    })
                    .count();
                assert!(count <= expected_commands, "Unexpected extra provider role");
                if count == expected_commands {
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "Real original result never reached exact startup + catalogue + six children"
                );
                std::thread::yield_now();
            }
            let provider = s
                .api
                .clone()
                .with_original_pr_read_bounds_v13(opening.deadline(), 1024 * 1024)
                .unwrap();
            let mut newer = base::open(
                opening.clone(),
                &provider,
                &database(&s.fixture),
                opening.cancellation(),
                opening.deadline(),
            )
            .unwrap();
            newer.error = "NEWER_OWNED_FORM_ERROR".into();
            let presentation = newer.presentation_v4().0.clone();
            s.app.form = Some(newer);
            let newer_sql = full_sql(&s.fixture);
            assert!(!owner.work().matches(&s.app));
            settle(&mut s, &mut background);
            let current = s.app.form.as_ref().unwrap();
            assert!(Arc::ptr_eq(current.presentation_v4().0, &presentation));
            assert_eq!(current.error, "NEWER_OWNED_FORM_ERROR");
            assert_eq!(full_sql(&s.fixture), newer_sql);
        } else {
            settle(&mut s, &mut background);
            if kind == "failed" {
                native::failed(&mut s.app, &owner);
                assert_eq!(
                    s.app.form.as_ref().unwrap().error,
                    "Conversation unavailable • draft/reference retained • nothing resent"
                );
                assert_eq!(s.app.form.as_ref().unwrap().error_scroll, 0);
            } else {
                assert!(
                    s.app.form.is_none() || s.app.form.as_ref().is_some_and(|f| f.error != REFUSAL)
                );
                assert_ne!(s.app.overlay, Some(Overlay::Form));
            }
            assert_eq!(full_sql(&s.fixture), sql);
        }
        calls_and_git(&s, s.steps.len());
    }
}
