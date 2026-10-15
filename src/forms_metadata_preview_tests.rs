use super::*;
use crate::{app::Overlay, pr_metadata_preview::PrMetadataPreview};
use serde_json::json;

fn form(kind: PrFormKind) -> Form {
    let target: PrSummary = serde_json::from_value(json!({
        "pr":{"host":"github.invalid","id":"PR_form_7","repository":"example/repo","number":7,"title":"Original title","body":"Original description","url":"https://github.invalid/example/repo/pull/7","author":{"login":"author","actor_type":"User"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","queues":[]},
        "head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0
    })).unwrap();
    Form::pr(PrIntent {
        kind,
        target,
        identity: HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "offline".into(),
        },
    })
    .unwrap()
}
fn prepare(form: &mut Form) {
    let FormPurpose::Pr(intent) = &form.purpose else {
        panic!("genuine PR form")
    };
    form.metadata_preview = Some(PrMetadataPreview::prepare(intent, &form.values()).unwrap());
    form.preview = true;
    form.preview_requested = false;
}
fn edit(form: &mut Form, value: &str) {
    let before = form.fields[0].editor.text.clone();
    form.fields[0].editor = TextBuffer::new(value).unwrap();
    form.field_edited("body", &before);
}

#[test]
fn pr_metadata_preview_form_requires_prepared_exact_effect_and_no_change_is_readable_disabled() {
    for kind in [PrFormKind::Title, PrFormKind::Description, PrFormKind::Base] {
        let mut form = form(kind);
        assert!(form.requires_preview());
        assert!(form.uses_prepared_preview());
        assert!(!form.plan_ready());
        form.dispatch(&FormAction::Submit);
        assert!(!form.submit_requested);
        assert!(form.error.contains("Choose Preview"));
        assert!(form.validate_metadata_preview().is_err());
        prepare(&mut form);
        assert!(!form.plan_ready());
        assert!(form
            .plan_lines()
            .iter()
            .any(|line| line.starts_with("No change")));
        form.dispatch(&FormAction::Submit);
        assert!(form.error.starts_with("No change"));
        assert!(!form.submit_requested);
        assert!(form
            .validate_metadata_preview()
            .unwrap_err()
            .starts_with("No change"));
    }
    for kind in [
        PrFormKind::Comment,
        PrFormKind::Approve,
        PrFormKind::RequestChanges,
        PrFormKind::ReviewComment,
    ] {
        let mut form = form(kind);
        assert!(!form.requires_preview());
        assert!(!form.uses_prepared_preview());
        form.dispatch(&FormAction::Submit);
        assert!(form.submit_requested);
        form.validate_metadata_preview().unwrap();
    }
}

#[test]
fn pr_metadata_preview_form_paste_keyboard_and_editor_hooks_invalidate_without_discarding_text() {
    for method in 0..3 {
        let mut form = form(PrFormKind::Description);
        edit(&mut form, "Requested description");
        prepare(&mut form);
        assert!(form.plan_ready());
        form.validate_metadata_preview().unwrap();
        form.dispatch(&FormAction::Preview); // Back to editing, not a restored approval.
        match method {
            0 => form.paste(" pasted"),
            1 => {
                form.key(KeyCode::Char('!'), KeyModifiers::NONE);
            }
            _ => edit(&mut form, "External editor result café"),
        }
        assert!(!form.preview);
        assert!(form.metadata_preview.is_none());
        assert!(!form.plan_ready());
        assert_ne!(form.values()["body"], "Requested description");
        form.dispatch(&FormAction::Submit);
        assert!(!form.submit_requested);
        assert!(form.validate_metadata_preview().is_err());
        prepare(&mut form);
        form.validate_metadata_preview().unwrap();
        form.dispatch(&FormAction::Submit);
        assert!(form.submit_requested);
    }
}

#[test]
fn pr_metadata_preview_form_worker_recheck_refuses_bypassed_input_and_original_scope_head_drift() {
    let mut original = form(PrFormKind::Title);
    edit(&mut original, "Requested title");
    prepare(&mut original);
    for change in 0..5 {
        let mut changed = original.clone();
        let FormPurpose::Pr(intent) = &mut changed.purpose else {
            panic!("PR")
        };
        match change {
            0 => changed.fields[0].editor = TextBuffer::new("Unreviewed replacement").unwrap(),
            1 => intent.identity.account_id = Some(43),
            2 => intent.target.head_sha = "c".repeat(40),
            3 => intent.target.base_sha = "d".repeat(40),
            _ => intent.target.pr.title = "Concurrent original title".into(),
        }
        assert!(
            changed.plan_ready(),
            "A cached-present UI hint is not worker authority"
        );
        assert!(changed
            .validate_metadata_preview()
            .unwrap_err()
            .contains("changed after Preview"));
    }
    original.validate_metadata_preview().unwrap();
}

#[test]
fn pr_metadata_preview_form_close_and_v2_draft_restore_never_restore_confirmation_approval() {
    let mut original = form(PrFormKind::Title);
    edit(&mut original, "Retained title café 雪");
    prepare(&mut original);
    let draft = original.draft();
    let data: serde_json::Value = serde_json::from_str(&draft).unwrap();
    assert_eq!(data["version"], 2);
    assert_eq!(data["values"]["body"], "Retained title café 雪");
    assert!(data["publication"].is_null());
    assert!(data.get("metadata_preview").is_none());
    assert!(data.get("fingerprint").is_none());
    let mut restored = form(PrFormKind::Title);
    edit(&mut restored, "Previously prepared unrelated input");
    prepare(&mut restored);
    restored.restore_draft(&draft).unwrap();
    assert_eq!(restored.values()["body"], "Retained title café 雪");
    assert!(restored.restored_draft);
    assert!(!restored.preview);
    assert!(restored.metadata_preview.is_none());
    assert!(restored.validate_metadata_preview().is_err());
    let mut app = App {
        form: Some(original),
        overlay: Some(Overlay::Form),
        ..Default::default()
    };
    app.dispatch(Action::Form(FormAction::Close));
    assert!(app.overlay.is_none());
    assert!(app.form_dismissed);
    let closed = app.form.unwrap();
    assert!(closed.metadata_preview.is_none());
    assert_eq!(closed.values()["body"], "Retained title café 雪");
    assert!(closed.validate_metadata_preview().is_err());
}

#[test]
fn pr_metadata_preview_form_layout_has_real_confirm_hits_readable_no_color_and_no_field_overlap() {
    for (width, height) in [(120, 40), (80, 24)] {
        let mut form = form(PrFormKind::Title);
        edit(&mut form, "Requested title");
        prepare(&mut form);
        // The genuine prepared metadata model now crosses the SAME worker-only
        // presentation boundary before this isolated renderer contract.
        form.prepare_preview_display_v1(
            &Default::default(),
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .unwrap();
        let mut app = App {
            form: Some(form),
            overlay: Some(Overlay::Form),
            no_color: true,
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
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("PR metadata preview"));
        assert!(text.contains("Target: example/repo#7"));
        assert!(text.contains("Caller: @octocat"));
        assert!(text.contains("[Confirm action]"));
        assert!(!text.contains("[Submit]"));
        assert!(app
            .hits
            .iter()
            .any(|hit| hit.action == Action::Form(FormAction::Submit) && !hit.rect.is_empty()));
        assert!(!app
            .hits
            .iter()
            .any(|hit| matches!(hit.action, Action::Form(FormAction::Field(_)))));
        assert!(app
            .form
            .as_ref()
            .unwrap()
            .field_rects
            .iter()
            .all(|rect| rect.is_empty()));
        app.form
            .as_mut()
            .unwrap()
            .key(KeyCode::PageDown, KeyModifiers::NONE);
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Current Title"));
        assert!(text.contains("Original title"));
        assert!(text.contains("Requested Title"));
        assert!(text.contains("Requested title"));
    }
}
