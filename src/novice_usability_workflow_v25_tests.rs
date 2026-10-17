//! Genuine source/Core/Preview/Confirm plus actual SQLite receipts. Display
//! assertions never supply source, current permission, history or approval.
use super::*;
use crate::{
    app::{Action, View},
    scoped_actions_native_v1::publication_v3 as publication,
    scoped_actions_workflow_ui_v6::{self as display, WorkflowOutcomeFeedbackV25 as Feedback},
    theme::{ColorMode, SemanticToken},
};
use ratatui::{backend::TestBackend, buffer::Buffer, style::Color, Terminal};

struct Actual {
    // Join the real draft worker before its disposable fixture directory drops.
    _writer: crate::drafts::DraftWriter,
    fixture: Fixture,
    provider: GithubProvider,
    app: App,
    steps: Vec<GhStep>,
}
fn actual(after: &str) -> Actual {
    let (fixture, provider, mut app, steps) =
        prepared("active", attempt("active", after, false, 42), false);
    let (value, owner, writer) = confirm(&mut app, &provider, &fixture);
    assert!(publication::publish(&mut app, &owner, value));
    app.scoped_actions_owner_v1 = None;
    Actual {
        fixture,
        provider,
        app,
        steps,
        _writer: writer,
    }
}
fn sql_all(fixture: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        f::database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let tables=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut out = Vec::new();
    for table in tables {
        let mut s = db
            .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
            .unwrap();
        let n = s.column_count();
        let mut rows = s
            .query_map([], |r| {
                Ok(format!(
                    "{table}:{:?}",
                    (0..n)
                        .map(|i| r.get::<_, rusqlite::types::Value>(i))
                        .collect::<Result<Vec<_>, _>>()?
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        out.extend(rows);
    }
    out
}
fn paint(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        // The production top-level paint clears/replaces the preceding frame's
        // hit regions before drawing this same actual owned Form.
        .draw(|frame| crate::ui::draw(frame, app))
        .unwrap();
    terminal.backend().buffer().clone()
}
fn rows(buffer: &Buffer) -> String {
    buffer
        .content
        .chunks(usize::from(buffer.area.width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
fn feedback_style(app: &mut App, expected: Feedback) {
    for mode in [
        ColorMode::TrueColor,
        ColorMode::LowColor,
        ColorMode::NoColor,
    ] {
        app.no_color = mode == ColorMode::NoColor;
        app.color_mode = mode;
        for (width, height) in [(80, 20), (80, 24), (120, 40)] {
            let buffer = paint(app, width, height);
            let text = rows(&buffer);
            let form = app.form.as_ref().unwrap();
            assert_eq!(display::feedback_v25(form), Some(expected));
            assert!(
                text.contains(expected.label()),
                "Explicit outcome must survive no-color: {text}"
            );
            assert!(text.contains("[F7 Info↑]"));
            assert!(text.contains("[F8 Info↓]"));
            assert!(text.contains("Tab: buttons • Esc: close"));
            assert!(text.contains("[F4 Acknowledge locally]"));
            assert!(
                !text.contains("Ctrl-A: select all")
                    && !text.contains("Ctrl-P: preview")
                    && !text.contains("Esc: keep draft")
            );
            assert!(
                text.find(display::result_heading_v42(form)).unwrap()
                    < text.find("Disable workflow • entire repository").unwrap()
            );
            assert!(!text.contains("[F7 Error↑]"));
            assert!(!text.contains("Some("));
            assert!(!text.contains("None"));
            assert!(text.contains("@octocat"));
            assert!(text.contains("account #42"));
            let area = form.error_rect;
            assert_eq!(
                buffer[(area.x, area.y)].fg,
                app.theme()
                    .style(expected.token())
                    .fg
                    .unwrap_or(Color::Reset)
            );
            assert_eq!(
                app.hits
                    .iter()
                    .filter(|hit| matches!(hit.action, Action::Form(FormAction::ErrorScroll(_))))
                    .count(),
                2
            );
        }
    }
}

#[test]
fn novice_usability_workflow_v25_genuine_confirmed_result_readable_actor_semantics_and_scroll_80_120_color_no_color(
) {
    let mut a = actual("disabled_manually");
    assert_eq!(w::result_sql(&a.fixture)[0].0, "confirmed");
    let sql = sql_all(&a.fixture);
    let git = f::git_state(&a.fixture);
    let reference = a.app.form.as_ref().unwrap().actions_publication.clone();
    feedback_style(&mut a.app, Feedback::Confirmed);
    let opened = opened(&a.app);
    assert!(opened
        .lines()
        .iter()
        .any(|s| s == "Disable workflow • entire repository • all refs"));
    assert!(opened
        .lines()
        .iter()
        .any(|s| s.contains("NOT only this YAML, PR, commit, ref or run")));
    assert!(opened
        .lines()
        .iter()
        .any(|s| s.contains("observed state is not causation")));
    assert!(opened.retained() <= 8 * 1024 * 1024);
    for (key, delta) in [
        (crossterm::event::KeyCode::F(8), 3),
        (crossterm::event::KeyCode::F(7), -3),
    ] {
        let action = a
            .app
            .form
            .as_mut()
            .unwrap()
            .key(key, crossterm::event::KeyModifiers::NONE)
            .unwrap();
        assert_eq!(action, FormAction::ErrorScroll(delta));
        a.app.dispatch(Action::Form(action));
    }
    paint(&mut a.app, 80, 24);
    assert_eq!(a.app.form.as_ref().unwrap().actions_publication, reference);
    assert_eq!(sql_all(&a.fixture), sql);
    assert_eq!(f::git_state(&a.fixture), git);
    w::exact(&a.fixture, &a.steps);
}

#[test]
fn novice_usability_workflow_v25_actual_accepted_pending_is_waiting_not_success_with_same_durable_blocker(
) {
    let mut a = actual("active");
    assert_eq!(w::result_sql(&a.fixture)[0].0, "accepted_pending");
    let sql = sql_all(&a.fixture);
    let git = f::git_state(&a.fixture);
    feedback_style(&mut a.app, Feedback::AcceptedPending);
    assert_eq!(
        display::feedback_v25(a.app.form.as_ref().unwrap())
            .unwrap()
            .token(),
        SemanticToken::Pending
    );
    assert!(a.app.form.as_ref().unwrap().error.contains("NOT observed"));
    assert!(!a.app.form.as_ref().unwrap().plan_ready());
    assert_eq!(sql_all(&a.fixture), sql);
    assert_eq!(f::git_state(&a.fixture), git);
    w::exact(&a.fixture, &a.steps);
}

#[test]
fn novice_usability_workflow_v25_actual_local_ack_error_ref_mismatch_and_unissued_http_text_never_inherit_success(
) {
    let mut a = actual("disabled_manually");
    paint(&mut a.app, 80, 24);
    let sql = sql_all(&a.fixture);
    let git = f::git_state(&a.fixture);
    let reference = a.app.form.as_ref().unwrap().actions_publication.clone();
    a.app
        .dispatch(Action::Form(FormAction::AcknowledgeActionsV3));
    assert!(a
        .app
        .form
        .as_ref()
        .unwrap()
        .error
        .contains("Inspect exact original workflow state before ACK"));
    assert!(display::feedback_v25(a.app.form.as_ref().unwrap()).is_none());
    let buffer = paint(&mut a.app, 80, 24);
    let text = rows(&buffer);
    assert!(text.contains("[F7 Error↑]"));
    assert!(!text.contains("[F7 Info↑]"));
    let area = a.app.form.as_ref().unwrap().error_rect;
    assert_eq!(
        buffer[(area.x, area.y)].fg,
        a.app
            .theme()
            .style(SemanticToken::Failure)
            .fg
            .unwrap_or(Color::Reset)
    );
    assert_eq!(a.app.form.as_ref().unwrap().actions_publication, reference);
    // Diagnostic fault injection changes ONLY the display reference; it does
    // not mint a source/history/ACK proof or issue another worker request.
    let form = a.app.form.as_mut().unwrap();
    form.error = Feedback::Confirmed.message().into();
    assert_eq!(display::feedback_v25(form), Some(Feedback::Confirmed));
    form.actions_publication.as_mut().unwrap().attempt_id += 1;
    assert!(display::feedback_v25(form).is_none());
    form.actions_publication = reference;
    assert_eq!(sql_all(&a.fixture), sql);
    assert_eq!(f::git_state(&a.fixture), git);
    w::exact(&a.fixture, &a.steps);
    let (fix, _p, mut fresh, all) = prepared("active", vec![], false);
    let before = sql_all(&fix);
    let source = fresh.remote_scope_source.as_ref().unwrap().clone();
    fresh.form.as_mut().unwrap().error = Feedback::Confirmed.message().into();
    assert!(
        display::feedback_v25(fresh.form.as_ref().unwrap()).is_none(),
        "Even exact HTTP204 text without an actual typed result cannot create success"
    );
    assert!(rows(&paint(&mut fresh, 80, 24)).contains("[F7 Error↑]"));
    assert!(Arc::ptr_eq(
        fresh.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert_eq!(sql_all(&fix), before);
    w::exact(&fix, &all);
}

#[test]
fn novice_usability_workflow_v25_genuine_saved_receipt_and_optional_account_captions_are_honest_data_only(
) {
    let a = actual("disabled_manually");
    let sql = sql_all(&a.fixture);
    let git = f::git_state(&a.fixture);
    let identity = opened(&a.app).identity().clone();
    let reference = a
        .app
        .form
        .as_ref()
        .unwrap()
        .actions_publication
        .as_ref()
        .unwrap();
    let journal = ActionsWriteJournal::open(&f::database(&a.fixture)).unwrap();
    let marker = journal
        .original_publication(&a.provider, &identity, reference)
        .unwrap();
    let lines = crate::actions_routes::original_lines(&marker).join("\n");
    assert!(lines.contains("Saved workflow change #1 • Disable workflow"));
    assert!(lines.contains(&format!("Saved workflow: #7 • repository {}", f::REPO)));
    assert!(lines.contains("Recorded result: workflow state • workflow #7 • Run attempt: not applicable to workflow state"));
    assert!(!lines.contains("Some("));
    assert!(!lines.contains("None"));
    assert!(!lines.contains("Workflow {"));
    assert!(lines.contains("Original synchronous effect confirmed and durably recorded"));
    assert!(lines.contains("Nothing is resent"));
    // Pure formatting negative ONLY. This altered copy is never validated,
    // installed, persisted, or used as a current/historical capability.
    let mut missing_display_only = marker.clone();
    missing_display_only.receipt.as_mut().unwrap().resource_id = None;
    let missing = crate::actions_routes::original_lines(&missing_display_only).join("\n");
    assert!(missing.contains("workflow ID not reported"));
    assert!(!missing.contains("workflow #0"));
    assert_eq!(marker.receipt.as_ref().unwrap().resource_id, Some(7));
    assert_eq!(display::account_caption_v25(Some(42)), "#42");
    assert_eq!(display::account_caption_v25(None), "ID unavailable");
    assert_eq!(display::account_caption_v25(Some(0)), "ID unavailable");
    assert_eq!(a.app.view, View::Actions);
    assert_eq!(sql_all(&a.fixture), sql);
    assert_eq!(f::git_state(&a.fixture), git);
    w::exact(&a.fixture, &a.steps);
}
