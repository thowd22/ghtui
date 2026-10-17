//! Genuine Core -> shared Form/TestBackend/actual input -> same Main autosave
//! helper and real SQLite ACK. These supplement, never replace, required PTYs.
use super::*;
use crate::{
    app::{Action, Overlay},
    forms::FormAction,
    pr_metadata_picker::PrMetadataPickerAction,
    scoped_pr_collaboration_ui_v17::{self as ui, issue_link_v87 as issue_ui},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::time::Duration;
#[path = "entry_v102_tests.rs"]
mod entry_v102;
fn paint(app: &mut crate::app::App, cols: u16, rows: u16) {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(cols, rows)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
}
fn click(app: &mut crate::app::App, action: &FormAction) {
    let hit = app
        .hits
        .iter()
        .find(|hit| hit.action == Action::Form(action.clone()))
        .expect("Actual owned current control");
    let (column, row) = (
        hit.rect.x + hit.rect.width.saturating_sub(1) / 2,
        hit.rect.y + hit.rect.height.saturating_sub(1) / 2,
    );
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}
fn key(app: &mut crate::app::App, code: KeyCode) {
    app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}
fn prepared_ui(
    mouse: bool,
) -> (
    personal::Setup,
    Arc<crate::scoped_pr_issue_links_v87::IssueLinkPreviewV87>,
) {
    prepared_ui_with_tail(mouse, Vec::new())
}
fn prepared_ui_with_tail(
    mouse: bool,
    tail: Vec<GhStep>,
) -> (
    personal::Setup,
    Arc<crate::scoped_pr_issue_links_v87::IssueLinkPreviewV87>,
) {
    let mut calls = open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    );
    calls.extend(selected_steps(pair(pr_node(), issue(9))));
    calls.extend(tail);
    let mut s = personal::setup(calls);
    let original = PrCollaborationOpenIntentV17::capture_issue_links_v87(
        &s.app,
        &personal::database(&s.fixture),
        IssueLinkCommandV87::LinkIssue,
    )
    .unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let form = issue_ui::open(
        Arc::clone(&original),
        IssueLinkCommandV87::LinkIssue,
        &provider,
        &personal::database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    s.app.form = Some(form);
    s.app.overlay = Some(Overlay::Form);
    s.app.form_dismissed = false;
    s.app.color_mode = crate::theme::ColorMode::NoColor;
    paint(&mut s.app, 120, 40);
    if mouse {
        click(&mut s.app, &FormAction::OpenPicker(0));
    } else {
        key(&mut s.app, KeyCode::F(2));
    }
    assert!(issue_ui::picker(s.app.form.as_ref().unwrap()).is_some());
    paint(&mut s.app, 120, 40);
    if mouse {
        click(
            &mut s.app,
            &FormAction::MetadataPicker(PrMetadataPickerAction::IndexedRow(0)),
        );
        paint(&mut s.app, 120, 40);
        click(
            &mut s.app,
            &FormAction::MetadataPicker(PrMetadataPickerAction::Choose),
        );
    } else {
        for ch in "Actual Issue 9".chars() {
            key(&mut s.app, KeyCode::Char(ch));
        }
        paint(&mut s.app, 120, 40);
        key(&mut s.app, KeyCode::Enter);
    }
    let form = s.app.form.as_ref().unwrap();
    assert!(issue_ui::picker(form).is_none());
    assert!(!form.dirty);
    let request = issue_ui::request(form).unwrap();
    assert!(request.preview.is_none());
    assert_eq!(state_command(&request), IssueLinkCommandV87::LinkIssue);
    let count = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .count();
    assert_eq!(
        count,
        personal::opening().len() + 11,
        "Picker/filter/Choose creates NO child"
    );
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let bounded = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let preview = request
        .prepare(
            &bounded,
            &personal::database(&s.fixture),
            &cancel,
            deadline,
            false,
        )
        .unwrap_or_else(|e| panic!("{}", e.message));
    assert!(issue_ui::accept_preview(
        s.app.form.as_mut().unwrap(),
        &request,
        Arc::clone(&preview)
    ));
    paint(&mut s.app, 120, 40);
    assert!(issue_ui::request(s.app.form.as_ref().unwrap())
        .unwrap()
        .matches_confirm(s.app.form.as_ref().unwrap()));
    (s, preview)
}
fn state_command(request: &issue_ui::IssueLinkRequestV87) -> IssueLinkCommandV87 {
    request.intent.command
}
#[test]
fn pr_metadata_issue_link_v87_actual_picker_key_mouse_no_color_preview_and_main_initial_save_keep_exact_ack_not_permission(
) {
    for mouse in [false, true] {
        let (mut s, preview) = prepared_ui(mouse);
        let form = s.app.form.as_ref().unwrap();
        assert!(issue_ui::pending_initial_save(form));
        let before = personal::sql(&s.fixture);
        let before_reservations = reservations(&s);
        let before_schema = schema_sql(&s);
        let mut writer = crate::drafts::DraftWriter::new(personal::database(&s.fixture));
        let form = s.app.form.as_mut().unwrap();
        let revision = issue_ui::state(form).unwrap().revision;
        let epoch = form.presentation_v4().1;
        form.prepare_draft_autosave_v77();
        assert_eq!(issue_ui::state(form).unwrap().revision, revision);
        assert_eq!(form.presentation_v4().1, epoch);
        let body = ui::encoded_draft(form).unwrap();
        let result = writer.save(form.scope(), form.resource(), body.clone());
        form.settle_draft_autosave_v77(result.is_ok());
        result.unwrap();
        assert!(!form.dirty);
        assert!(!issue_ui::pending_initial_save(form));
        assert!(Arc::ptr_eq(
            issue_ui::state(form).unwrap().preview.as_ref().unwrap(),
            &preview
        ));
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
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
        assert_eq!(
            crate::storage::Cache::open(&personal::database(&s.fixture))
                .unwrap()
                .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
                .unwrap()
                .as_deref(),
            Some(body.as_str())
        );
        let db = rusqlite::Connection::open_with_flags(
            personal::database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let stamp: i64 = db
            .query_row(
                "SELECT updated_at FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
                rusqlite::params![form.scope().host, form.scope().login, form.resource()],
                |r| r.get(0),
            )
            .unwrap();
        let mut expected = before;
        use rusqlite::types::ValueRef;
        expected.push(format!(
            "drafts:{:?}",
            vec![
                format!("{:?}", ValueRef::Text(b"github.invalid")),
                format!("{:?}", ValueRef::Text(b"user-id:42")),
                format!("{:?}", ValueRef::Text(form.resource().as_bytes())),
                format!("{:?}", ValueRef::Text(body.as_bytes())),
                format!("{:?}", ValueRef::Integer(stamp))
            ]
        ));
        expected.sort();
        let mut actual = personal::sql(&s.fixture);
        actual.sort();
        assert_eq!(
            actual, expected,
            "Only exact known ordinary draft row is added; no publication/derived change"
        );
        assert_eq!(reservations(&s), before_reservations);
        assert_eq!(schema_sql(&s), before_schema);
        s.app.handle_event(Event::Resize(40, 10));
        paint(&mut s.app, 40, 10);
        assert!(!issue_ui::request(s.app.form.as_ref().unwrap())
            .unwrap()
            .matches_confirm(s.app.form.as_ref().unwrap()));
        s.app.handle_event(Event::Resize(120, 40));
        paint(&mut s.app, 120, 40);
        assert!(issue_ui::request(s.app.form.as_ref().unwrap())
            .unwrap()
            .matches_confirm(s.app.form.as_ref().unwrap()));
        transport_and_git(&s);
    }
}
#[test]
fn pr_metadata_issue_link_v87_changed_raw_value_foreign_form_and_failed_initial_registration_retire_only_approval(
) {
    for fault in 0..3 {
        let (s, preview) = prepared_ui(false);
        let mut form = s.app.form.as_ref().unwrap().clone();
        let raw = form.fields[0].editor.text.clone();
        let before = personal::sql(&s.fixture);
        match fault {
            0 => {
                form.fields[0].editor =
                    crate::editor::TextBuffer::new("different desired Issue").unwrap()
            }
            1 => {
                let mut foreign = crate::forms::Form::pr(crate::forms::PrIntent {
                    kind: crate::forms::PrFormKind::Title,
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
            2 => form.settle_draft_autosave_v77(false),
            _ => unreachable!(),
        }
        assert!(!issue_ui::pending_initial_save(&form));
        form.prepare_draft_autosave_v77();
        assert!(!form.preview);
        assert!(issue_ui::state(&form).unwrap().preview.is_none());
        assert!(!preview.useful.load(std::sync::atomic::Ordering::Acquire));
        if fault != 0 {
            assert_eq!(form.fields[0].editor.text, raw);
        }
        assert_eq!(personal::sql(&s.fixture), before);
        transport_and_git(&s);
    }
}
