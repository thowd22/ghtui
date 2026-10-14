use super::*;
use crate::{
    actions_workspace::{ActionsCommand, ActionsWorkspace},
    actions_write_journal::ActionsPublicationReference,
};

fn form() -> Form {
    Form::actions(
        ActionsWorkspace::new(
            HostIdentity {
                host: "github.invalid".into(),
                login: "octocat".into(),
                account_id: Some(42),
                gh_version: "offline".into(),
            },
            vec!["example/repo".into()],
        ),
        ActionsCommand::SetSecret,
    )
    .unwrap()
}
fn reference() -> ActionsPublicationReference {
    ActionsPublicationReference {
        attempt_id: 17,
        fingerprint_sha256: "a".repeat(64),
    }
}

#[test]
fn actions_run_control_draft_v3_restores_original_reference_before_empty_secret_plan_validation() {
    let mut original = form();
    original
        .fields
        .iter_mut()
        .find(|field| field.key == "name")
        .unwrap()
        .editor = TextBuffer::new("ORIGINAL_SECRET").unwrap();
    original
        .fields
        .iter_mut()
        .find(|field| field.secret)
        .unwrap()
        .editor = TextBuffer::new("transient-never-saved").unwrap();
    original.actions_publication = Some(reference());
    let body = original.draft();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["version"], 3);
    assert_eq!(parsed["purpose"], "actions");
    assert!(!body.contains("transient-never-saved"));
    assert!(!format!("{original:?}").contains("transient-never-saved"));
    let mut restored = form();
    restored.restore_draft(&body).unwrap();
    assert_eq!(restored.actions_publication, Some(reference()));
    assert!(restored.publication.is_none());
    assert!(restored.restored_draft);
    assert!(restored
        .fields
        .iter()
        .find(|field| field.secret)
        .unwrap()
        .editor
        .text
        .is_empty());
    assert!(!restored.plan_ready());
    restored.dispatch(&FormAction::Submit);
    assert!(
        restored.submit_requested,
        "Viewing an original must not require an empty-secret plan"
    );
}

#[test]
fn actions_run_control_draft_rejects_wrong_version_purpose_and_reference_atomically() {
    let mut restored = form();
    restored
        .fields
        .iter_mut()
        .find(|field| field.key == "name")
        .unwrap()
        .editor = TextBuffer::new("unchanged").unwrap();
    let valid = serde_json::json!({"version":3,"purpose":"actions","values":{"name":"injected"},"actions_publication":reference()});
    for mutation in [
        serde_json::json!({"version":2,"purpose":"actions","values":{"name":"injected"}}),
        serde_json::json!({"version":3,"purpose":"pr","values":{"name":"injected"}}),
        serde_json::json!({"version":3,"purpose":"actions","values":{"name":"injected"},"actions_publication":{"attempt_id":0,"fingerprint_sha256":"a".repeat(64)}}),
        serde_json::json!({"version":3,"purpose":"actions","values":{"name":"injected"},"actions_publication":{"attempt_id":17,"fingerprint_sha256":"g".repeat(64)}}),
    ] {
        assert!(restored.restore_draft(&mutation.to_string()).is_err());
        assert_eq!(restored.values()["name"], "unchanged");
        assert!(restored.actions_publication.is_none());
        assert!(!restored.restored_draft);
    }
    restored.restore_draft(&valid.to_string()).unwrap();
    assert_eq!(restored.values()["name"], "injected");
    restored
        .restore_draft(r#"{"name":"legacy","value":"ignored-secret"}"#)
        .unwrap();
    assert_eq!(restored.values()["name"], "legacy");
    assert!(restored.actions_publication.is_none());
    assert!(restored
        .fields
        .iter()
        .find(|field| field.secret)
        .unwrap()
        .editor
        .text
        .is_empty());
    let mut local = Form::compare(crate::comparison::CompareIntent {
        directory: "/fixture/local".into(),
        base: crate::comparison::CompareEndpoint::manual("a".repeat(40)),
        head: crate::comparison::CompareEndpoint::manual("b".repeat(40)),
        mode: crate::comparison::ComparisonMode::Snapshot,
    })
    .unwrap();
    assert!(local.restore_draft(&valid.to_string()).is_err());
}

#[test]
fn actions_run_control_edits_preserve_original_and_new_intent_is_explicit_local_only() {
    let mut draft = form();
    draft.actions_publication = Some(reference());
    draft.actions_original_lines = vec!["Original target: unchanged".into()];
    draft.restored_draft = true;
    let name = draft
        .fields
        .iter()
        .position(|field| field.key == "name")
        .unwrap();
    draft.dispatch(&FormAction::Field(name));
    draft.paste("new-name");
    assert_eq!(draft.actions_publication, Some(reference()));
    assert_eq!(draft.actions_original_lines, ["Original target: unchanged"]);
    assert!(draft.draft().contains("fingerprint_sha256"));
    draft.submitted = true;
    draft.preview = true;
    let action = draft.key(KeyCode::F(9), KeyModifiers::NONE).unwrap();
    assert_eq!(action, FormAction::NewIntent);
    draft.dispatch(&action);
    assert!(draft.actions_publication.is_none());
    assert!(draft.actions_original_lines.is_empty());
    assert!(!draft.submitted && !draft.restored_draft && !draft.preview);
    assert!(draft.dirty);
    assert!(!draft.submit_requested);
    assert!(draft.error.contains("durable protection remains"));
    draft.dispatch(&FormAction::Submit);
    assert!(!draft.submit_requested);
    assert!(draft.error.contains("Preview"));
}

#[test]
fn actions_run_control_original_new_intent_has_same_keyboard_and_mouse_layout_action() {
    for (width, height) in [(80, 24), (120, 40)] {
        let mut original = form();
        original.actions_publication = Some(reference());
        original.actions_original_lines = vec![
            "ORIGINAL ACTIONS ATTEMPT #17".into(),
            "GitHub accepted • completion not confirmed".into(),
        ];
        original.preview = true;
        let mut app = App {
            form: Some(original),
            overlay: Some(crate::app::Overlay::Form),
            ..Default::default()
        };
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("[F9 New intent]"));
        assert!(text.contains("[View original]"));
        assert!(!text.contains("[Confirm action]"));
        let hit = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::Form(FormAction::NewIntent))
            .unwrap()
            .clone();
        assert!(hit.rect.width > 0 && hit.rect.y < height);
        let mut keyboard = app.form.as_ref().unwrap().clone();
        keyboard.focus = keyboard.fields.len()
            + keyboard
                .footer_actions()
                .iter()
                .position(|action| *action == FormAction::NewIntent)
                .unwrap();
        assert_eq!(
            keyboard.key(KeyCode::Enter, KeyModifiers::NONE),
            Some(FormAction::NewIntent)
        );
        keyboard.dispatch(&FormAction::NewIntent);
        app.dispatch(hit.action);
        assert_eq!(
            app.form.as_ref().unwrap().actions_publication,
            keyboard.actions_publication
        );
        assert_eq!(app.form.as_ref().unwrap().draft(), keyboard.draft());
        assert!(!app.form.as_ref().unwrap().submit_requested);
    }
}
