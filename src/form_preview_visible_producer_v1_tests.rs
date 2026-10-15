//! Genuine closed Git/image/UID/inline producer -> shared Form presentation.
//! SOURCE ONLY: these declarations need Root's nonzero code and PTY gates.
use super::*;
use crate::{
    app::{Action, Overlay},
    prepared_form_text_v1::PreparedFormTextV1,
};

fn prepared(
    body: &str,
) -> (
    Setup,
    Form,
    Arc<ui::InlineComposerRequestV1>,
    Arc<ScopedInlinePreviewV1>,
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(source_fence_steps(h, true));
        steps.extend(image_steps(h));
    });
    let mut form = form(&mut s);
    form.key(KeyCode::Char('a'), KeyModifiers::CONTROL);
    form.paste(body);
    let request = ui::request(&form).unwrap();
    assert_eq!(request.body().as_ref(), body);
    let preview = preview(&s, &request, request.body().clone());
    assert!(ui::accept_preview(&mut form, &request, preview.clone()));
    let current_request = ui::request(&form).unwrap();
    (s, form, current_request, preview)
}
fn app(s: &Setup, form: Form) -> App {
    App {
        view: crate::app::View::Inbox,
        remote_scope_source: Some(s.source.clone()),
        remote_scope: Some(s.active.clone()),
        github: Some(s.workspace.clone()),
        form: Some(form),
        overlay: Some(Overlay::Form),
        ..Default::default()
    }
}
fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| crate::forms::draw(frame, app, frame.area()))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
fn sql(s: &Setup) -> Vec<(String, String)> {
    let database = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = database
        .prepare("SELECT resource,body FROM drafts ORDER BY host,login,resource")
        .unwrap();
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}
fn fixture_effects(s: &Setup, calls: usize, drafts: &[(String, String)]) {
    assert_eq!(commands(&s.fixture).len(), calls);
    assert_eq!(sql(s), drafts);
    unchanged_git_and_exact_script(s);
}
fn long_body() -> String {
    (0..160)
        .map(|row| format!("ROW{row:03} {} 界e\u{301}\n", "abcdefgh".repeat(13)))
        .collect()
}

#[test]
fn form_preview_visible_producer_document_is_exact_original_worker_arc_not_foreground_reconstruction(
) {
    let (s, form, request, preview) = prepared("Exact body 界\nsecond original line");
    let document = form.preview_display_v1.document.as_ref().unwrap();
    assert!(Arc::ptr_eq(document, preview.display_v1()));
    assert!(Arc::ptr_eq(
        &ui::confirmed_preview(&form).unwrap(),
        &preview
    ));
    assert!(request.matches_form(&form));
    assert!(document.retained_bytes() > 0 && document.retained_bytes() <= 16 * 1024 * 1024);
    assert!(form
        .plan_lines()
        .iter()
        .any(|line| line == "second original line"));
    unchanged_git_and_exact_script(&s);
}
#[test]
fn form_preview_visible_producer_repeated_deep_draw_keeps_same_document_zero_gh_sql_git_effects() {
    let (s, form, request, preview) = prepared(&long_body());
    let document = preview.display_v1().clone();
    let mut app = app(&s, form);
    let calls = commands(&s.fixture).len();
    let drafts = sql(&s);
    draw(&mut app, 120, 40);
    app.form.as_mut().unwrap().preview_scroll = 120;
    let first = draw(&mut app, 120, 40);
    assert!(first.contains("ROW"));
    for _ in 0..20 {
        assert_eq!(draw(&mut app, 120, 40), first);
    }
    assert!(Arc::ptr_eq(
        app.form
            .as_ref()
            .unwrap()
            .preview_display_v1
            .document
            .as_ref()
            .unwrap(),
        &document
    ));
    assert!(request.matches_form(app.form.as_ref().unwrap()));
    fixture_effects(&s, calls, &drafts);
}
#[test]
fn form_preview_visible_producer_width_switch_preserves_actual_safe_byte_anchor_without_reads_or_renewal(
) {
    let (s, form, request, preview) = prepared(&long_body());
    let mut app = app(&s, form);
    let calls = commands(&s.fixture).len();
    let drafts = sql(&s);
    draw(&mut app, 80, 24);
    app.form.as_mut().unwrap().preview_scroll = 160;
    draw(&mut app, 80, 24);
    let top = app.form.as_ref().unwrap().preview_scroll;
    for _ in 0..10 {
        draw(&mut app, 120, 40);
        draw(&mut app, 80, 24);
        assert_eq!(app.form.as_ref().unwrap().preview_scroll, top);
    }
    assert!(Arc::ptr_eq(
        app.form
            .as_ref()
            .unwrap()
            .preview_display_v1
            .document
            .as_ref()
            .unwrap(),
        preview.display_v1()
    ));
    assert!(request.matches_form(app.form.as_ref().unwrap()));
    fixture_effects(&s, calls, &drafts);
}
#[test]
fn form_preview_visible_producer_tiny_confirm_keyboard_and_click_share_actual_disabled_target() {
    let (s, form, _, _) = prepared("Visible exact body before any confirmation");
    let mut app = app(&s, form);
    let calls = commands(&s.fixture).len();
    let drafts = sql(&s);
    let text = draw(&mut app, 40, 24);
    assert!(text.contains("resize required"));
    assert!(app
        .hits
        .iter()
        .any(|hit| matches!(hit.action, Action::Form(FormAction::Preview))));
    assert!(app
        .hits
        .iter()
        .any(|hit| matches!(hit.action, Action::Form(FormAction::Close))));
    let click = app
        .hits
        .iter()
        .find_map(|hit| match &hit.action {
            Action::Form(FormAction::Submit) => Some(hit.action.clone()),
            _ => None,
        })
        .unwrap();
    let form = app.form.as_mut().unwrap();
    let action = form.key(KeyCode::Enter, KeyModifiers::CONTROL).unwrap();
    form.dispatch(&action);
    assert!(!form.submit_requested);
    assert!(form.error.contains("Resize"));
    if let Action::Form(action) = click {
        form.dispatch(&action);
    } else {
        unreachable!()
    }
    assert!(!form.submit_requested);
    assert!(form.error.contains("Resize"));
    fixture_effects(&s, calls, &drafts);
}
#[test]
fn form_preview_visible_producer_input_resize_rejects_previous_paint_until_actual_useful_redraw() {
    let (s, form, request, _) = prepared("Original target/body must remain inspectable");
    let mut app = app(&s, form);
    draw(&mut app, 120, 40);
    let form = app.form.as_mut().unwrap();
    assert!(!form.preview_display_v1.geometry_rejected);
    form.clear_preview_display_geometry_v1();
    form.dispatch(&FormAction::Submit);
    assert!(!form.submit_requested);
    assert!(request.matches_form(form));
    draw(&mut app, 40, 24);
    assert!(
        app.form
            .as_ref()
            .unwrap()
            .preview_display_v1
            .geometry_rejected
    );
    draw(&mut app, 120, 40);
    let form = app.form.as_mut().unwrap();
    assert!(!form.preview_display_v1.geometry_rejected);
    form.dispatch(&FormAction::Submit);
    assert!(form.submit_requested); // a request, NEVER an API call/write in paint
    unchanged_git_and_exact_script(&s);
}
#[test]
fn form_preview_visible_producer_edit_retires_display_and_cannot_restore_old_request_by_equal_body()
{
    let (s, mut form, request, preview) = prepared("Original body");
    form.dispatch(&FormAction::Preview);
    form.key(KeyCode::Char('!'), KeyModifiers::NONE);
    form.key(KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(form.fields[0].editor.text, "Original body");
    assert!(form.preview_display_v1.document.is_none());
    assert!(!request.matches_form(&form));
    assert!(!ui::accept_preview(&mut form, &request, preview));
    assert!(!form.preview);
    assert!(!form.submit_requested);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn form_preview_visible_producer_same_document_has_no_source_or_publication_authority_and_cancel_cannot_prepare(
) {
    let (s, form, _, preview) = prepared("No display authority");
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(PreparedFormTextV1::prepare(
        form.plan_lines(),
        &cancel,
        Instant::now() + Duration::from_secs(30)
    )
    .is_err());
    assert!(!form.submit_requested);
    assert_eq!(preview.body(), "No display authority");
    assert!(!format!("{:?}", preview.display_v1()).contains("No display authority"));
    unchanged_git_and_exact_script(&s);
}
#[test]
fn form_preview_visible_producer_control_escape_and_wrapped_rows_keep_original_body_and_both_source_oids(
) {
    let body = "Control\t tab \u{202e} Bidi\nUnicode 界e\u{301}";
    let (s, form, request, preview) = prepared(body);
    let mut app = app(&s, form);
    let painted = draw(&mut app, 120, 40);
    assert_eq!(request.body().as_ref(), body);
    assert_eq!(preview.body(), body);
    assert!(!painted.contains('\u{1b}'));
    assert!(!painted.contains('\u{202e}'));
    assert!(preview.lines().iter().any(|line| line.contains(&s.h.h)));
    assert!(preview.lines().iter().any(|line| line.contains(&s.h.m)));
    assert!(Arc::ptr_eq(
        app.form
            .as_ref()
            .unwrap()
            .preview_display_v1
            .document
            .as_ref()
            .unwrap(),
        preview.display_v1()
    ));
    unchanged_git_and_exact_script(&s);
}
