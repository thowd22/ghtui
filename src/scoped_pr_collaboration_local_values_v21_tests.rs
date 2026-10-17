//! Genuine source factories; local desired values are never current authority.
use super::*;
use crate::{
    app::{Action, Overlay, View},
    drafts::DraftWriter,
    forms::{Form, FormAction},
    runtime::{scoped_inline_test_support_v1 as channel, Background},
    scoped_pr_collaboration_native_v17 as native,
};
use crossterm::event::{KeyCode, KeyModifiers};
const COMMANDS: [PrCommand; 6] = [
    PrCommand::NewComment,
    PrCommand::EditComment,
    PrCommand::DeleteComment,
    PrCommand::AddReaction,
    PrCommand::RemoveReaction,
    PrCommand::Subscription,
];

fn actual(command: PrCommand, preview: bool) -> (Setup, Form) {
    let mut calls = phase(command, true, Some(true));
    if preview {
        calls.extend(phase(command, false, Some(true)));
    }
    let s = fixture(command, calls);
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let opened = OpenedConversationDraftV17::open(
        original.clone(),
        command,
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    (s, ui::form_from_draft(opened))
}
fn desired(command: PrCommand) -> BTreeMap<String, String> {
    match command {
        PrCommand::NewComment | PrCommand::EditComment => {
            BTreeMap::from([("body".into(), TEXT.into())])
        }
        PrCommand::AddReaction => BTreeMap::from([("reaction".into(), "heart".into())]),
        PrCommand::RemoveReaction => BTreeMap::from([("reaction_id".into(), "601".into())]),
        PrCommand::Subscription => BTreeMap::from([("subscription".into(), "ignored".into())]),
        PrCommand::DeleteComment => BTreeMap::new(),
        _ => unreachable!(),
    }
}
fn all_sql(fixture: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut out = Vec::new();
    for name in names {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |r| {
                let fields = (0..columns)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("{name}:{fields:?}"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        out.extend(rows);
    }
    out
}

#[test]
fn conversation_comments_v21_same_borrowed_parser_actual_six_intents_exact_utf8_and_owned_selection_no_effects(
) {
    for command in COMMANDS {
        let (s, form) = actual(command, false);
        let request = ui::request(&form).unwrap();
        let intent = request.intent();
        let values = desired(command);
        let validate = |values: &BTreeMap<String, String>,
                        selected: Option<&crate::pr_actions::SelectedComment>,
                        account: Option<u64>| {
            PrOperation::validate_values_v17(command, selected, &intent.reactions, account, values)
        };
        let sql = all_sql(&s.fixture);
        let commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        assert!(validate(&values, intent.selected.as_ref(), Some(42)).is_ok());
        let operation = PrOperation::from_values_v17(
            command,
            intent.selected.as_ref(),
            &intent.reactions,
            Some(42),
            &values,
        )
        .unwrap();
        assert_eq!(operation.command_v17(), command);
        let mut invalid = Vec::new();
        match command {
            PrCommand::NewComment | PrCommand::EditComment => {
                for text in [
                    "".to_owned(),
                    " \n\t ".to_owned(),
                    format!("{}ab", "界".repeat(21_845)),
                ] {
                    invalid.push(BTreeMap::from([("body".into(), text)]));
                }
                let exact = format!("{}a", "界".repeat(21_845));
                assert_eq!(exact.len(), 65_536);
                let bounded = BTreeMap::from([("body".into(), exact.clone())]);
                assert!(validate(&bounded, intent.selected.as_ref(), Some(42)).is_ok());
                match PrOperation::from_values_v17(
                    command,
                    intent.selected.as_ref(),
                    &intent.reactions,
                    Some(42),
                    &bounded,
                )
                .unwrap()
                {
                    PrOperation::NewComment(body) | PrOperation::EditComment(_, body) => {
                        assert_eq!(body.as_bytes(), exact.as_bytes())
                    }
                    _ => unreachable!(),
                }
            }
            PrCommand::AddReaction => {
                for value in ["", "HEART", "not-a-reaction"] {
                    invalid.push(BTreeMap::from([("reaction".into(), value.into())]));
                }
            }
            PrCommand::RemoveReaction => {
                for value in ["", "602", "0601", "999999999999999999999999"] {
                    invalid.push(BTreeMap::from([("reaction_id".into(), value.into())]));
                }
                for account in [None, Some(0), Some(43)] {
                    assert!(validate(&values, intent.selected.as_ref(), account).is_err());
                }
            }
            PrCommand::Subscription => {
                for value in ["", "SUBSCRIBED", "not-a-state"] {
                    invalid.push(BTreeMap::from([("subscription".into(), value.into())]));
                }
            }
            PrCommand::DeleteComment => assert!(values.is_empty()),
            _ => unreachable!(),
        }
        for values in invalid {
            let pure = validate(&values, intent.selected.as_ref(), Some(42)).unwrap_err();
            let backend = PrOperation::from_values_v17(
                command,
                intent.selected.as_ref(),
                &intent.reactions,
                Some(42),
                &values,
            )
            .unwrap_err();
            assert_eq!(pure, backend);
        }
        if !matches!(command, PrCommand::NewComment | PrCommand::Subscription) {
            assert_eq!(
                validate(&values, None, Some(42)).unwrap_err(),
                "Select a comment first."
            );
        }
        assert_eq!(all_sql(&s.fixture), sql);
        assert_eq!(
            fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
            commands
        );
        exact_read(&s);
    }
}

#[test]
fn conversation_comments_v21_actual_local_invalid_keeps_form_source_slot_then_corrected_six_previews(
) {
    for command in COMMANDS {
        let (mut s, mut form) = actual(command, true);
        let before = ui::request(&form).unwrap();
        let source = before.intent().source.clone();
        let mut background = Background::new();
        if command != PrCommand::DeleteComment {
            assert_eq!(form.key(KeyCode::Char('u'), KeyModifiers::CONTROL), None);
            let invalid = if matches!(command, PrCommand::NewComment | PrCommand::EditComment) {
                "   "
            } else if command == PrCommand::RemoveReaction {
                "602"
            } else {
                "invalid"
            };
            for ch in invalid.chars() {
                assert_eq!(form.key(KeyCode::Char(ch), KeyModifiers::NONE), None);
            }
        }
        s.app.form = Some(form);
        s.app.overlay = Some(Overlay::Form);
        let page_allocations = allocations(&s.app);
        let sql = all_sql(&s.fixture);
        let commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        let loading = s.app.loading;
        let status = s.app.status.clone();
        if command != PrCommand::DeleteComment {
            s.app.dispatch(Action::Form(FormAction::Preview));
            let epoch = s.app.form.as_ref().unwrap().presentation_v4().1;
            native::prepare_requested(&mut s.app);
            let form = s.app.form.as_ref().unwrap();
            assert!(before.owns_presentation(form));
            assert_eq!(form.presentation_v4().1, epoch);
            assert!(!form.error.is_empty());
            assert!(
                !form.submitting
                    && !form.preview
                    && !form.submit_requested
                    && form.preview_requested
            );
            assert!(
                s.app.scoped_pr_collaboration_work_v17.is_none()
                    && s.app.scoped_pr_collaboration_owner_v17.is_none()
            );
            assert_eq!(channel::generation(&background), None);
            assert_eq!(s.app.loading, loading);
            assert_eq!(s.app.status, status);
            assert_eq!(allocations(&s.app), page_allocations);
            assert!(source.owns_presentation(&s.app));
            assert_eq!(all_sql(&s.fixture), sql);
            assert_eq!(
                fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
                commands
            );
            if matches!(command, PrCommand::NewComment | PrCommand::EditComment) {
                ui::replace_editor_result(s.app.form.as_mut().unwrap(), TEXT).unwrap();
            } else {
                let valid = desired(command).into_values().next().unwrap();
                let form = s.app.form.as_ref().unwrap();
                let index = form.fields[0]
                    .choices
                    .iter()
                    .position(|v| v == &valid)
                    .unwrap();
                s.app.dispatch(Action::Form(FormAction::Choice(0, index)));
            }
        }
        s.app.dispatch(Action::Form(FormAction::Preview));
        native::prepare_requested(&mut s.app);
        let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
        assert_eq!(work.phase(), native::ConversationPhaseKindV17::Preview);
        let writer = DraftWriter::new(database(&s.fixture));
        assert!(background.submit_scoped_pr_collaboration_v17(
            &mut s.app,
            work,
            s.api.clone(),
            database(&s.fixture),
            writer.client()
        ));
        let until = Instant::now() + Duration::from_secs(8);
        while s.app.loading[View::Inbox.index()] {
            background.poll(&mut s.app);
            assert!(
                Instant::now() < until,
                "Actual corrected same-intent Preview did not finish"
            );
            std::thread::yield_now();
        }
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut s.app))
            .unwrap();
        let form = s.app.form.as_ref().unwrap();
        assert!(form.preview && form.error.is_empty());
        let request = ui::request(form).unwrap();
        assert!(Arc::ptr_eq(&request.intent().source, &source));
        assert_eq!(request.values().as_ref(), &desired(command));
        assert!(ui::confirmed_preview(form).is_ok());
        assert_eq!(allocations(&s.app), page_allocations);
        exact_read(&s);
    }
}
