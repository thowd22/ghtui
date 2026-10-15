//! Genuine closed image/draft fixtures and the SAME shared native editor.
use super::*;
use crate::forms::{Form, FormAction, FormPurpose};
use crossterm::event::{KeyCode, KeyModifiers};
use images::inline_v1::ui_v1 as ui;

pub(super) fn form(s: &mut Setup) -> Form {
    let result = read(s, &Default::default(), &Default::default()).unwrap();
    let proof = result.proof.clone();
    let mut owner = App {
        view: crate::app::View::Inbox,
        remote_scope_source: Some(s.source.clone()),
        remote_scope: Some(s.active.clone()),
        github: Some(s.workspace.clone()),
        ..Default::default()
    };
    images::install_images(&mut owner, result).unwrap();
    s.workspace = owner.github.take().unwrap();
    let intent =
        ScopedInlineIntentV1::capture(&proof.canonical_range(PrImageSideV1::Old, 1, 2).unwrap())
            .unwrap();
    let key = intent.draft_key().unwrap();
    Cache::open(&database(&s.fixture))
        .unwrap()
        .put_draft(
            intent.scope(),
            &key,
            &json!({"version":2,"values":{"body":INLINE_BODY},"publication":null}).to_string(),
        )
        .unwrap();
    let draft = ScopedInlineDraftV1::open(
        intent,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    ui::form_from_draft(draft).unwrap()
}
pub(super) fn preview(
    s: &Setup,
    request: &ui::InlineComposerRequestV1,
    body: Arc<str>,
) -> Arc<ScopedInlinePreviewV1> {
    ScopedInlinePreviewV1::prepare(
        request.intent().clone(),
        body,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}
#[test]
fn inline_comments_ui_closed_identity_prepared_key_exact_body_ticket_without_restored_approval() {
    let mut s = inline_setup(|h, steps| steps.extend(source_fence_steps(h, true)));
    let mut form = form(&mut s);
    let request = ui::request(&form).unwrap();
    assert_eq!(request.body().as_ref(), INLINE_BODY);
    assert_eq!(form.resource(), request.intent().draft_key().unwrap());
    assert_eq!(form.scope(), *request.intent().scope());
    let FormPurpose::PrInline(state) = &form.purpose else {
        panic!("Actual inline purpose required")
    };
    assert_eq!(state.identity().login, "inline-publisher");
    assert_eq!(state.identity().account_id, Some(42));
    assert!(request.matches_form(&form));
    assert!(!form.plan_ready());
    assert!(ui::confirmed_preview(&form).is_err());
    assert!(
        !format!("{state:?} {request:?}").contains("Retained original source-bound inline body"),
        "New scheduling/presentation Debug never prints private body"
    );
    assert!(form
        .restore_draft("{malformed untrusted generic restore")
        .is_err());
    assert_eq!(form.fields[0].editor.text, INLINE_BODY);
    assert!(!form.submit_requested);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_ui_preview_requires_exact_opaque_request_and_any_edit_retires_approval() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(source_fence_steps(h, true));
        steps.extend(image_steps(h));
        steps.extend(image_steps(h));
    });
    let mut form = form(&mut s);
    let original = ui::request(&form).unwrap();
    let wrong = preview(
        &s,
        &original,
        Arc::from("Different genuine worker preview body"),
    );
    assert!(!ui::accept_preview(&mut form, &original, wrong));
    assert!(!form.plan_ready());
    let exact = preview(&s, &original, original.body().clone());
    assert!(ui::accept_preview(&mut form, &original, exact.clone()));
    assert!(
        form.plan_lines()
            .iter()
            .any(|line| line.contains(INLINE_BODY)),
        "Fresh preview must show the actual exact body, not only metadata/byte count"
    );
    assert!(Arc::ptr_eq(&ui::confirmed_preview(&form).unwrap(), &exact));
    let confirmed = ui::request(&form).unwrap();
    assert!(Arc::ptr_eq(confirmed.preview().unwrap(), &exact));
    form.dispatch(&FormAction::Preview); // Return to shared editing, not a write.
    form.key(KeyCode::Char('界'), KeyModifiers::NONE);
    form.key(KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(form.fields[0].editor.text, INLINE_BODY);
    assert!(
        !confirmed.matches_form(&form),
        "Edit-away/back cannot restore an old approval"
    );
    assert!(!original.matches_form(&form));
    assert!(ui::confirmed_preview(&form).is_err());
    assert!(!ui::accept_preview(&mut form, &original, exact));
    unchanged_git_and_exact_script(&s);
}

#[test]
fn inline_comments_ui_equal_original_intent_and_body_cannot_adopt_another_open_presentation() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(source_fence_steps(h, true));
        steps.extend(source_fence_steps(h, true));
        steps.extend(image_steps(h));
    });
    let mut first = form(&mut s);
    let original = ui::request(&first).unwrap();
    let draft = ScopedInlineDraftV1::open(
        original.intent().clone(),
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let mut second = ui::form_from_draft(draft).unwrap();
    let other = ui::request(&second).unwrap();
    assert!(Arc::ptr_eq(original.intent(), other.intent()));
    assert_eq!(original.body(), other.body());
    assert!(!original.matches_form(&second));
    assert!(!other.matches_form(&first));
    let prepared = preview(&s, &original, original.body().clone());
    assert!(!ui::accept_preview(
        &mut second,
        &original,
        prepared.clone()
    ));
    assert!(ui::accept_preview(&mut first, &original, prepared));
    assert!(!second.plan_ready());
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_ui_inline_limit_precedes_paste_key_request_and_external_editor_materialization()
{
    let mut s = inline_setup(|h, steps| steps.extend(source_fence_steps(h, true)));
    let mut form = form(&mut s);
    ui::replace_editor_result(&mut form, &"x".repeat(65_536)).unwrap();
    let exact = form.fields[0].editor.text.clone();
    form.paste("界");
    assert_eq!(form.fields[0].editor.text, exact);
    form.key(KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(form.fields[0].editor.text, exact);
    assert!(ui::replace_editor_result(&mut form, &"x".repeat(65_537)).is_err());
    assert_eq!(form.fields[0].editor.text, exact);
    assert!(ui::request(&form).is_ok());
    assert!(ui::encoded_draft(&form).unwrap().len() <= 128 * 1024);
    ui::replace_editor_result(&mut form, &format!("{}x", "\t".repeat(65_535))).unwrap();
    assert!(
        ui::encoded_draft(&form).is_err(),
        "Escaped JSON envelope cap is independent of raw body bytes"
    );
    assert!(
        ui::request(&form).is_err(),
        "No body Arc/preview request for unrecoverable encoded envelope"
    );
    form.fields[0].editor.select_all();
    form.paste("界e\u{301}\nretained");
    assert_eq!(form.fields[0].editor.text, "界e\u{301}\nretained");
    let request = ui::request(&form).unwrap();
    ui::replace_editor_result(&mut form, "界e\u{301}\nretained").unwrap();
    assert!(
        !request.matches_form(&form),
        "Even identical editor restore retires earlier requests"
    );
    form.fields[0].editor.text = "x".repeat(65_537); // Malformed in-memory caller, not an allowed UI edit.
    assert!(ui::request(&form).is_err());
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_ui_wide_narrow_shared_actions_avatar_and_owned_anchor_scroll_back() {
    let mut s = inline_setup(|h, steps| steps.extend(source_fence_steps(h, true)));
    let form = form(&mut s);
    let mut app = App {
        view: crate::app::View::Inbox,
        remote_scope_source: Some(s.source.clone()),
        remote_scope: Some(s.active.clone()),
        github: Some(s.workspace.clone()),
        form: Some(form),
        ..Default::default()
    };
    for (width, height) in [(120, 40), (80, 24)] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        app.hits.clear();
        terminal
            .draw(|frame| {
                let area = frame.area();
                crate::forms::draw(frame, &mut app, area)
            })
            .unwrap();
        let initial = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(initial.contains("@inline-publisher"));
        assert!(
            initial.contains("[?]"),
            "Unknown actor type remains honest shared avatar fallback at {width}: {initial}"
        );
        assert!(
            !initial.contains("[?]l@"),
            "No stale underlying header cell under shared4-cell avatar"
        );
        for action in [
            FormAction::Preview,
            FormAction::Editor,
            FormAction::Submit,
            FormAction::Close,
            FormAction::InlineAnchor,
        ] {
            assert!(
                app.hits
                    .iter()
                    .any(|hit| hit.action == crate::app::Action::Form(action.clone())
                        && hit.rect.width > 0
                        && hit.rect.right() <= width),
                "Actual visible hit for {action:?} at {width}"
            );
        }
        let form = app.form.as_mut().unwrap();
        assert_eq!(
            form.key(KeyCode::F(2), KeyModifiers::NONE),
            Some(FormAction::InlineAnchor)
        );
        form.dispatch(&FormAction::InlineAnchor);
        let original = form.fields[0].editor.text.clone();
        form.paste("must not reach covered editor");
        form.key(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(form.fields[0].editor.text, original);
        app.hits.clear();
        terminal
            .draw(|frame| {
                let area = frame.area();
                crate::forms::draw(frame, &mut app, area)
            })
            .unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(screen.contains("Original inline anchor"));
        assert!(screen.contains("Canonical API filename:"));
        assert!(screen.contains("Selected image path:"));
        assert!(screen.contains(&s.h.h));
        assert!(screen.contains(&s.h.m));
        let form = app.form.as_mut().unwrap();
        form.dispatch(&FormAction::InlineAnchorScroll(i32::MAX));
        terminal
            .draw(|frame| {
                let area = frame.area();
                crate::forms::draw(frame, &mut app, area)
            })
            .unwrap();
        assert!(app.form.as_ref().unwrap().field_rects.is_empty());
        let form = app.form.as_mut().unwrap();
        assert_eq!(
            form.key(KeyCode::Esc, KeyModifiers::NONE),
            Some(FormAction::InlineAnchor)
        );
        form.dispatch(&FormAction::InlineAnchor);
        assert!(!ui::anchor_open(form));
        assert_eq!(form.fields[0].editor.text, original);
        form.submitting = true;
        let theme = app.theme();
        terminal
            .draw(|frame| {
                let area = frame.area();
                crate::forms::draw(frame, &mut app, area)
            })
            .unwrap();
        let anchor = app
            .hits
            .iter()
            .find(|hit| hit.action == crate::app::Action::Form(FormAction::InlineAnchor))
            .unwrap();
        assert_eq!(
            terminal.backend().buffer()[(anchor.rect.x, anchor.rect.y)].fg,
            theme
                .style(crate::theme::SemanticToken::Muted)
                .fg
                .unwrap_or(ratatui::style::Color::Reset)
        );
        let actual = terminal.backend().buffer()[(anchor.rect.x, anchor.rect.y)].modifier;
        assert!(actual.contains(theme.style(crate::theme::SemanticToken::Muted).add_modifier));
        assert!(!actual.contains(ratatui::style::Modifier::UNDERLINED),"Disabled anchor cannot inherit the active Focus underline; shared parent BOLD is independent");
        assert_eq!(
            app.form
                .as_mut()
                .unwrap()
                .key(KeyCode::F(2), KeyModifiers::NONE),
            None
        );
        app.dispatch(crate::app::Action::Form(FormAction::InlineAnchor));
        let form = app.form.as_mut().unwrap();
        assert!(!ui::anchor_open(form));
        assert_eq!(form.fields[0].editor.text, original);
        assert_eq!(
            form.key(KeyCode::Esc, KeyModifiers::NONE),
            Some(FormAction::Close)
        );
        form.submitting = false;
    }
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_ui_pending_preview_paste_cannot_edit_body_revision_or_saved_intent() {
    let mut s = inline_setup(|h, steps| steps.extend(source_fence_steps(h, true)));
    let mut form = form(&mut s);
    let request = ui::request(&form).unwrap();
    assert!(!form.preview);
    form.submitting = true;
    form.paste("PASTE_MUST_NOT_EDIT_AN_INFLIGHT_PREVIEW");
    assert_eq!(
        form.fields[0].editor.text, INLINE_BODY,
        "Submitting Preview disables paste just like keyboard editing and dispatch"
    );
    assert!(request.matches_form(&form));
    assert!(!form.dirty);
    unchanged_git_and_exact_script(&s);
}
