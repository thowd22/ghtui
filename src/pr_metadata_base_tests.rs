use super::*;
use crate::{
    forms::{Form, FormPurpose, PrFormKind, PrIntent},
    pr_metadata_base::PrBaseSelection,
    pr_metadata_preview::PrMetadataPreview,
};

#[test]
fn pr_metadata_base_nullable_archive_cannot_inherit_old_false_and_oversized_branch_is_not_an_eligible_native_target(
) {
    let fixture = Fixture::new();
    let mut calls = protocol(vec![page(
        "branches",
        1,
        vec![json!({"name":"release","commit":{"sha":"c".repeat(40)}})],
    )]);
    let mut raw = repository();
    raw["archived"] = Value::Null;
    calls[5].stdout = serde_json::to_vec(&raw).unwrap();
    fixture.scripted_gh(&calls);
    let error = load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap_err();
    assert!(error.message.contains("archive state is unverified"));
    read_only(&fixture, 8);
    let fixture = Fixture::new();
    let raw_name = "a".repeat(1025);
    fixture.scripted_gh(&protocol(vec![page(
        "branches",
        1,
        vec![json!({"name":raw_name,"commit":{"sha":"c".repeat(40)}})],
    )]));
    let loaded = load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap();
    assert!(
        loaded.choose(0).is_err(),
        "An observed row beyond the exact-read native cap cannot grant a usable publication target"
    );
    read_only(&fixture, 8);
}

#[test]
fn pr_metadata_base_bound_choice_native_preview_requires_genuine_member_and_full_tip_without_any_preview_io(
) {
    let fixture = Fixture::new();
    fixture.scripted_gh(&protocol(vec![page(
        "branches",
        1,
        vec![json!({"name":"release","commit":{"sha":"c".repeat(40)}})],
    )]));
    let source =
        std::sync::Arc::new(load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap());
    let (identity, target, _) = original();
    let intent = PrIntent {
        kind: PrFormKind::Base,
        identity,
        target,
    };
    let values = std::collections::BTreeMap::from([("body".into(), "release".into())]);
    assert!(PrMetadataPreview::prepare_selected(&intent, &values, None)
        .unwrap_err()
        .contains("Choose a verified Base"));
    let selected = PrBaseSelection::new(source, 0, &intent).unwrap();
    let plan = PrMetadataPreview::prepare_selected(&intent, &values, Some(&selected)).unwrap();
    assert!(plan
        .preview()
        .iter()
        .any(|row| row == &format!("Requested base tip: {}", "c".repeat(40))));
    assert_eq!(plan.edit().base.as_deref(), Some("release"));
    plan.validate_selected(&intent, &values, Some(&selected))
        .unwrap();
    assert!(plan.validate_selected(&intent, &values, None).is_err());
    let mut changed = intent.clone();
    changed.target.base_ref = "another".into();
    assert!(selected.validate(&changed, &values).is_err());
    let mut changed = intent.clone();
    changed.identity.account_id = Some(43);
    assert!(selected.validate(&changed, &values).is_err());
    read_only(&fixture, 8);
}
#[test]
fn pr_metadata_base_form_raw_draft_edit_do_not_restore_genuine_chooser_or_preview_approval() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&protocol(vec![page(
        "branches",
        1,
        vec![json!({"name":"release","commit":{"sha":"c".repeat(40)}})],
    )]));
    let source =
        std::sync::Arc::new(load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap());
    let (identity, target, _) = original();
    let intent = PrIntent {
        kind: PrFormKind::Base,
        identity,
        target,
    };
    let selected = PrBaseSelection::new(source, 0, &intent).unwrap();
    let mut form = Form::pr(intent).unwrap();
    form.fields[0].editor = crate::editor::TextBuffer::new("release").unwrap();
    form.base_selection = Some(selected);
    let FormPurpose::Pr(intent) = &form.purpose else {
        unreachable!()
    };
    form.metadata_preview = Some(
        PrMetadataPreview::prepare_selected(intent, &form.values(), form.base_selection.as_ref())
            .unwrap(),
    );
    form.validate_metadata_preview().unwrap();
    let draft = form.draft();
    let saved: Value = serde_json::from_str(&draft).unwrap();
    assert_eq!(saved["values"], json!({"body":"release"}));
    assert_eq!(saved.as_object().unwrap().len(), 3);
    let mut restored = form.clone();
    restored.restore_draft(&draft).unwrap();
    assert!(restored.base_selection.is_none() && restored.metadata_preview.is_none());
    assert_eq!(restored.values()["body"], "release");
    form.fields[0].editor = crate::editor::TextBuffer::new("manual-replacement").unwrap();
    form.field_edited("body", "release");
    assert!(form.base_selection.is_none() && form.metadata_preview.is_none());
    assert!(form.validate_metadata_preview().is_err());
    read_only(&fixture, 8);
}
#[test]
fn pr_metadata_base_fresh_exact_branch_read_changed_tip_refuses_before_any_journal_claim() {
    let fixture = Fixture::new();
    let mut steps = protocol(vec![page(
        "branches",
        1,
        vec![json!({"name":"release","commit":{"sha":"c".repeat(40)}})],
    )]);
    let mut exact = protocol(vec![get(
        "repos/example/repo/branches/release",
        json!({"name":"release","commit":{"sha":"d".repeat(40)}}),
    )]);
    exact.push(GhStep::read(&["--version"], "gh version 2.101.0\n"));
    exact.push(get("user", json!({"id":42,"login":"requestor"})));
    steps.extend(exact);
    fixture.scripted_gh(&steps);
    let source =
        std::sync::Arc::new(load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap());
    let (identity, target, _) = original();
    let intent = PrIntent {
        kind: PrFormKind::Base,
        identity,
        target,
    };
    let selected = PrBaseSelection::new(source, 0, &intent).unwrap();
    let mut form = Form::pr(intent).unwrap();
    form.fields[0].editor = crate::editor::TextBuffer::new("release").unwrap();
    form.base_selection = Some(selected);
    let refused = crate::pr_metadata_base::revalidate(
        &form,
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(refused
        .status
        .contains("Original indexed choice is absent/changed/unavailable"));
    assert!(refused.form.unwrap().metadata_preview.is_none());
    let actual = commands(&fixture);
    assert_eq!(actual.len(), 18);
    assert!(actual.iter().all(|row| row["matched"] == true));
    assert!(actual
        .iter()
        .all(|row| row["args"][0] == "--version" || row["args"][5] == "GET"));
    assert!(
        !fixture.root.join("cache.sqlite3").exists(),
        "Read-only refusal never opens/claims a mutation ledger"
    );
}
