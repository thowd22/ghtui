//! Genuine source-produced Draft/Preview, supplementary to actual PTY flows.
//! DECLARED/UNRUN until the coordinator serially verifies this source cohort.
use super::*;
#[path = "scoped_pr_thread_draft_witness_v1_tests.rs"]
mod draft_witness_v1;
use crate::{
    forms::FormPurpose,
    scoped_pr_observed_tabs_v1::resources::threads_v1::publication_v1::{
        self as publish, ui_v1 as composer, ThreadPublicationActionV1,
    },
};

fn browser(extra: Vec<GhStep>) -> Real {
    let mut steps = thread_steps(Some("LINE"));
    steps.extend(extra);
    let mut real = real(steps);
    let prepared = read(&real).unwrap();
    let mut view = ui_v1::ThreadBrowserV1::pending(prepared.intent.clone());
    view.prepared = Some(prepared);
    real.app.scoped_threads_v1 = Some(view);
    real.app.overlay = Some(Overlay::PrThreadsV1);
    real
}
fn intent(
    real: &Real,
    action: ThreadPublicationActionV1,
) -> Arc<publish::ScopedThreadPublicationIntentV1> {
    publish::ScopedThreadPublicationIntentV1::capture(&real.app, action).unwrap()
}
fn draft(
    real: &Real,
    intent: Arc<publish::ScopedThreadPublicationIntentV1>,
) -> publish::ScopedThreadDraftV1 {
    publish::ScopedThreadDraftV1::open(
        intent,
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}
fn form_with_body(
    real: &Real,
    intent: Arc<publish::ScopedThreadPublicationIntentV1>,
    text: &str,
) -> crate::forms::Form {
    let mut form = composer::form_from_draft(draft(real, intent)).unwrap();
    form.paste(text);
    form
}
#[test]
fn review_threads_v1_composer_genuine_body_edit_and_identical_editor_restore_retire_exact_approval_request(
) {
    let real = browser(observation_steps(selected_node(false, true)));
    let intent = intent(&real, ThreadPublicationActionV1::Reply);
    let mut form = form_with_body(&real, intent, "original user reply");
    let request = composer::request(&form).unwrap();
    assert!(request.matches_form(&form));
    composer::replace_editor_result(&mut form, "original user reply").unwrap();
    assert!(!request.matches_form(&form));
    assert!(!form.plan_ready());
    assert!(composer::confirmed_preview(&form).is_err());
    assert_eq!(form.fields[0].editor.text, "original user reply");
    protocol(&real, 0);
}
#[test]
fn review_threads_v1_composer_input_editor_encoded_caps_and_busy_paste_preserve_original_bytes() {
    let real = browser(observation_steps(selected_node(false, true)));
    let intent = intent(&real, ThreadPublicationActionV1::Reply);
    let mut form = form_with_body(&real, intent, "saved body 界");
    let original = form.fields[0].editor.text.clone();
    form.paste(&"x".repeat(publish::BODY_CAP + 1));
    assert_eq!(form.fields[0].editor.text, original);
    assert!(composer::replace_editor_result(&mut form, &"界".repeat(publish::BODY_CAP)).is_err());
    assert_eq!(form.fields[0].editor.text, original);
    let request = composer::request(&form).unwrap();
    form.submitting = true;
    form.paste("must not edit held Preview");
    assert!(request.matches_form(&form));
    assert_eq!(form.fields[0].editor.text, original);
    form.submitting = false;
    composer::replace_editor_result(&mut form, &"\u{1}".repeat(publish::BODY_CAP)).unwrap();
    assert!(composer::encoded_draft(&form).is_err());
    assert!(composer::request(&form).is_err());
    assert_eq!(form.fields[0].editor.text.len(), publish::BODY_CAP);
    protocol(&real, 0);
}
#[test]
fn review_threads_v1_composer_different_real_draft_presentation_cannot_adopt_old_request_ticket() {
    let mut extra = observation_steps(selected_node(false, true));
    extra.extend(observation_steps(selected_node(false, true)));
    let real = browser(extra);
    let intent = intent(&real, ThreadPublicationActionV1::Reply);
    let first = form_with_body(&real, intent.clone(), "same actual text");
    let request = composer::request(&first).unwrap();
    let second = form_with_body(&real, intent, "same actual text");
    assert!(!request.matches_form(&second));
    assert!(request.matches_form(&first));
    protocol(&real, 0);
}
#[test]
fn review_threads_v1_composer_worker_preview_exact_body_original_root_and_current_actor_are_prepared(
) {
    let mut extra = observation_steps(selected_node(false, true));
    extra.extend(observation_steps(selected_node(false, true)));
    let real = browser(extra);
    let intent = intent(&real, ThreadPublicationActionV1::Reply);
    let original_body = "user reply \u{1b}[2J 界";
    let mut form = form_with_body(&real, intent, original_body);
    assert_eq!(form.fields[0].editor.text, "user reply [2J 界");
    // Pasting deliberately strips controls. Actual external-editor adoption
    // preserves its bytes, so use that production path to test safe preview.
    composer::replace_editor_result(&mut form, original_body).unwrap();
    let request = composer::request(&form).unwrap();
    assert_eq!(request.body().as_ref(), original_body);
    let preview = publish::ScopedThreadPreviewV1::prepare(
        request.intent().clone(),
        request.body().clone(),
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(preview.body(), original_body);
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains("Actual replyTo:null original comment #9009")));
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains(&"c".repeat(40)) && line.contains("NOT replaced by PR H")));
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains("\\u{1b}[2J")));
    assert!(!preview.lines().iter().any(|line| line.contains('\u{1b}')));
    assert!(composer::accept_preview(
        &mut form,
        &request,
        preview.clone()
    ));
    assert!(Arc::ptr_eq(
        &composer::confirmed_preview(&form).unwrap(),
        &preview
    ));
    assert!(
        matches!(&form.purpose,FormPurpose::PrThread(state) if state.identity().account_id==Some(42))
    );
    let approved_request = composer::request(&form).unwrap();
    assert!(approved_request.matches_form(&form));
    assert!(form.dirty, "Pending autosave is not a new body edit");
    form.paste("no edit while preview");
    assert!(approved_request.matches_form(&form));
    assert_eq!(form.fields[0].editor.text, original_body);
    assert!(Arc::ptr_eq(
        &composer::confirmed_preview(&form).unwrap(),
        &preview
    ));
    protocol(&real, 0);
}
#[test]
fn review_threads_v1_state_composer_has_no_comment_field_or_editor_and_explicit_preview_required() {
    let real = browser(observation_steps(selected_node(false, true)));
    let intent = intent(&real, ThreadPublicationActionV1::Resolve);
    let mut form = composer::form_from_draft(draft(&real, intent)).unwrap();
    assert!(form.fields.is_empty());
    assert!(composer::replace_editor_result(&mut form, "not sent").is_err());
    form.paste("no comment body");
    assert!(form.fields.is_empty());
    let request = composer::request(&form).unwrap();
    assert!(request.body().is_empty());
    assert!(composer::confirmed_preview(&form).is_err());
    assert!(!form.plan_ready());
    protocol(&real, 0);
}

#[test]
fn review_threads_v1_state_form_actual_render_click_hits_and_keyboard_footer_order_are_identical() {
    use crate::{app::Action, forms::FormAction};
    use crossterm::event::{KeyCode, KeyModifiers};
    for (width, height) in [(80, 24), (120, 40)] {
        let mut real = browser(observation_steps(selected_node(false, true)));
        let selected = intent(&real, ThreadPublicationActionV1::Resolve);
        let form = composer::form_from_draft(draft(&real, selected)).unwrap();
        assert!(
            form.dirty,
            "Untouched state intent registers a real normal autosave revision"
        );
        real.app.form = Some(form);
        real.app.overlay = Some(Overlay::Form);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut real.app))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!text.contains("[External editor]"));
        assert!(!real
            .app
            .hits
            .iter()
            .any(|hit| hit.action == Action::Form(FormAction::Editor)));
        for action in [FormAction::Preview, FormAction::Submit, FormAction::Close] {
            assert!(real
                .app
                .hits
                .iter()
                .any(|hit| hit.action == Action::Form(action.clone()) && !hit.rect.is_empty()));
        }
        let form = real.app.form.as_mut().unwrap();
        // Zero text fields: first Tab advances actual Preview to actual Submit,
        // without an invisible Editor slot or misleading visual focus.
        assert_eq!(form.key(KeyCode::Tab, KeyModifiers::NONE), None);
        assert_eq!(
            form.key(KeyCode::Enter, KeyModifiers::NONE),
            Some(FormAction::Submit)
        );
        form.key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(
            form.key(KeyCode::Enter, KeyModifiers::NONE),
            Some(FormAction::Close)
        );
        protocol(&real, 0);
    }
}

#[test]
fn review_threads_v1_actual_journal_reference_makes_body_inspect_only_without_trapping_focus_or_close(
) {
    use crate::forms::FormAction;
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut real = browser(observation_steps(selected_node(false, true)));
    let selected = intent(&real, ThreadPublicationActionV1::Reply);
    let draft = draft(&real, selected.clone());
    let identity = draft.identity().clone();
    let mut form = composer::form_from_draft(draft).unwrap();
    form.paste("actual original body remains protected");
    let mut journal =
        crate::pr_write_journal::PrWriteJournal::open(&database(&real.fixture)).unwrap();
    let target = crate::pr_write_journal::PrWriteTarget::new(
        &identity,
        real.app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .as_ref()
            .unwrap()
            .observed()
            .parent()
            .summary(),
    )
    .unwrap();
    let write = crate::pr_write_journal::PrWriteIntent {
        command: "thread-reply".into(),
        selected_resource: format!("thread-reply/{}", "a".repeat(64)),
        observed_head: selected.head().into(),
        values: std::collections::BTreeMap::from([(
            "body".into(),
            "actual original body remains protected".into(),
        )]),
    };
    let permit = journal.begin(&target, &write).unwrap();
    form.publication = Some(permit.marker().publication_reference());
    form.dirty = false;
    real.sql = sql_state(&real.fixture); // exact newly created durable fixture baseline
    let original = form.fields[0].editor.text.clone();
    let reference = form.publication.clone();
    for code in [
        KeyCode::Char('x'),
        KeyCode::Backspace,
        KeyCode::Delete,
        KeyCode::Enter,
    ] {
        assert_eq!(form.key(code, KeyModifiers::NONE), None);
        assert_eq!(form.fields[0].editor.text, original);
    }
    form.key(KeyCode::Char('u'), KeyModifiers::CONTROL);
    form.paste("not a new approved body");
    form.dispatch(&FormAction::Editor);
    assert!(!form.editor_requested);
    assert!(composer::replace_editor_result(&mut form, "not adopted").is_err());
    assert_eq!(form.fields[0].editor.text, original);
    assert_eq!(form.publication, reference);
    assert!(!form.dirty);
    assert!(composer::request(&form).is_err());
    form.key(KeyCode::Tab, KeyModifiers::NONE);
    assert!(
        form.focus > 0,
        "Inspect-only history still permits genuine keyboard navigation"
    );
    assert_eq!(
        form.key(KeyCode::Esc, KeyModifiers::NONE),
        Some(FormAction::Close)
    );
    protocol(&real, 0);
}

#[test]
fn review_threads_v1_actual_preview_completion_mixed_or_foreign_preserves_original_slot_form_and_loading(
) {
    for mode in [0, 1] {
        let mut extra = observation_steps(selected_node(false, true));
        extra.extend(observation_steps(selected_node(false, true)));
        let mut real = browser(extra);
        let selected = intent(&real, ThreadPublicationActionV1::Reply);
        let mut form = form_with_body(&real, selected, "real queued preview body");
        form.preview_requested = true;
        real.app.form = Some(form);
        real.app.overlay = Some(Overlay::Form);
        native_v1::capture_form(&mut real.app);
        let owner = real.app.scoped_threads_work_v1.take().unwrap();
        assert_eq!(owner.phase(), native_v1::ScopedThreadPhaseV1::Preview);
        let mut background = Background::new();
        assert!(background.submit_scoped_threads_v1(
            &mut real.app,
            owner.clone(),
            provider(&real.fixture),
            database(&real.fixture)
        ));
        channel::fault_next(&mut background, mode);
        let generation = channel::generation(&background);
        real.app.status = "EXACT_CURRENT_PREVIEW_STATUS".into();
        assert!(!background.poll(&mut real.app));
        assert_eq!(real.app.status, "EXACT_CURRENT_PREVIEW_STATUS");
        assert_eq!(channel::generation(&background), generation);
        assert!(Arc::ptr_eq(
            real.app.scoped_threads_owner_v1.as_ref().unwrap(),
            &owner
        ));
        assert!(real.app.loading[View::Inbox.index()]);
        let form = real.app.form.as_ref().unwrap();
        assert!(form.submitting);
        assert!(!form.plan_ready());
        assert_eq!(form.fields[0].editor.text, "real queued preview body");
        assert!(background.cancel_scoped_threads_v1(&mut real.app));
        assert!(!real.app.form.as_ref().unwrap().submitting);
        protocol(&real, 0);
    }
}

#[test]
fn review_threads_v1_actual_preview_success_error_newer_intent_veto_precedes_all_form_private_effects(
) {
    for mode in [2, 3] {
        let mut extra = observation_steps(selected_node(false, true));
        extra.extend(observation_steps(selected_node(false, true)));
        let mut real = browser(extra);
        let selected = intent(&real, ThreadPublicationActionV1::Reply);
        let mut form = form_with_body(&real, selected, "exact original preview text");
        form.preview_requested = true;
        real.app.form = Some(form);
        real.app.overlay = Some(Overlay::Form);
        native_v1::capture_form(&mut real.app);
        let owner = real.app.scoped_threads_work_v1.take().unwrap();
        let mut background = Background::new();
        assert!(background.submit_scoped_threads_v1(
            &mut real.app,
            owner,
            provider(&real.fixture),
            database(&real.fixture)
        ));
        channel::fault_next(&mut background, mode);
        let generation = channel::generation(&background);
        real.app.github_action = Some(GithubAction::Back);
        real.app.status = "NEWER_REAL_BACK_STATUS".into();
        assert!(!background.poll(&mut real.app));
        assert_eq!(channel::generation(&background), generation);
        assert_eq!(real.app.github_action, Some(GithubAction::Back));
        assert_eq!(real.app.status, "NEWER_REAL_BACK_STATUS");
        let form = real.app.form.as_ref().unwrap();
        assert!(form.submitting);
        assert!(!form.plan_ready());
        assert_eq!(form.fields[0].editor.text, "exact original preview text");
        real.app.github_action = None;
        assert!(background.cancel_scoped_threads_v1(&mut real.app));
        protocol(&real, 0);
    }
}

#[test]
fn review_threads_v1_direct_worker_preview_escaped_encoded_bound_refuses_before_any_new_read_or_body_materialization(
) {
    let real = browser(vec![]);
    let selected = intent(&real, ThreadPublicationActionV1::Reply);
    let error = publish::ScopedThreadPreviewV1::prepare(
        selected,
        Arc::from("\u{1}".repeat(publish::BODY_CAP)),
        &provider(&real.fixture),
        &database(&real.fixture),
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap_err();
    assert!(!error.hide_private);
    assert!(error.message().contains("128KiB"));
    let printable = "界".repeat(publish::BODY_CAP / 3);
    let encoded = publish::encode_draft(&printable, None).unwrap();
    assert!(encoded.len() <= publish::DRAFT_CAP);
    let actual: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(actual["values"]["body"], printable);
    protocol(&real, 0);
}
