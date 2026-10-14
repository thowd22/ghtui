//! Real factory-backed transient identity contracts.
use super::*;
use crate::{
    repository_catalogue_read_limits_test_support_v1 as support,
    repository_catalogue_workspace_v1::{CatalogueInput, CatalogueRequest},
    test_support::Fixture,
};
use serde_json::json;

fn workspace(source: Arc<RepositoryCatalogueRead>) -> CatalogueWorkspace {
    let (mut workspace, job) = CatalogueWorkspace::new(source).unwrap();
    workspace
        .accept(job.prepare(&CancellationToken::default()).unwrap())
        .unwrap();
    workspace
}
fn select(workspace: &mut CatalogueWorkspace, index: usize, pin: bool) {
    // Paste is insertion, not replacement; use the real editor selection.
    workspace.apply(CatalogueInput::SelectAll).unwrap();
    if let Some(CatalogueRequest::Prepare(job)) = workspace
        .apply(CatalogueInput::Paste(format!("repo{index:04}")))
        .unwrap()
    {
        workspace
            .accept(job.prepare(&CancellationToken::default()).unwrap())
            .unwrap();
    }
    assert_eq!(
        workspace
            .frame()
            .unwrap()
            .selected_target
            .as_ref()
            .unwrap()
            .id(),
        10000 + index as u64
    );
    workspace.apply(CatalogueInput::Select).unwrap();
    if pin {
        workspace.apply(CatalogueInput::Pin).unwrap();
    }
}
fn read(fixture: &Fixture) -> Arc<RepositoryCatalogueRead> {
    Arc::new(
        support::provider(fixture)
            .repository_catalogue_read(
                None,
                &crate::providers::gh::RepositoryCatalogueSource::Organization("example".into()),
                &crate::providers::gh::RepositoryCatalogueLimits::default(),
                &CancellationToken::default(),
            )
            .unwrap(),
    )
}

#[test]
fn repo_catalog_read_limits_transient_exact_ids_nodes_restore_without_rows_or_permissions() {
    let old_rows = [support::row(0), support::row(1)];
    let mut new_rows = old_rows.clone();
    new_rows[0]["description"] = json!("NEW_OBSERVATION_ONLY");
    new_rows[0]["permissions"]["pull"] = json!(false);
    let mut steps = support::collection(&old_rows, 20, 42, 42);
    steps.extend(support::collection(&new_rows, 20, 42, 42));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let old = support::load(
        &fixture,
        &crate::providers::gh::RepositoryCatalogueLimits::default(),
    );
    let mut original = workspace(old.source().clone());
    select(&mut original, 0, true);
    select(&mut original, 1, false);
    let snapshot = original.snapshot_transient_choices().unwrap();
    let prepared = snapshot.prepare(&CancellationToken::default()).unwrap();
    let fresh = read(&fixture);
    let restored = prepared
        .restore(fresh.clone(), &CancellationToken::default())
        .unwrap();
    assert_eq!(restored.query(), "repo0001");
    assert!(restored.unresolved().is_empty());
    let mut next = workspace(fresh.clone());
    next.restore_observed_pins(old.intent()).unwrap();
    restored.install(&mut next).unwrap();
    assert_eq!(next.selected_ids(), &[10000, 10001].into_iter().collect());
    assert!(next.pins().contains(&10000));
    assert_eq!(
        next.source().unwrap().repositories()[0].permissions.pull,
        Some(false),
        "Choice restore is not a permission carry/grant"
    );
    assert_eq!(
        next.source().unwrap().repositories()[0]
            .description
            .as_deref(),
        Some("NEW_OBSERVATION_ONLY")
    );
    assert_eq!(support::generation(&fixture, &old), 1);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_transient_missing_nodes_stay_unresolved_until_genuine_recovery() {
    let old_rows = [support::row(0)];
    let mut steps = support::collection(&old_rows, 20, 42, 42);
    steps.extend(support::collection(&[], 20, 42, 42));
    steps.extend(support::collection(&old_rows, 20, 42, 42));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let old = support::load(
        &fixture,
        &crate::providers::gh::RepositoryCatalogueLimits::default(),
    );
    let mut original = workspace(old.source().clone());
    select(&mut original, 0, true);
    let prepared = original
        .snapshot_transient_choices()
        .unwrap()
        .prepare(&CancellationToken::default())
        .unwrap();
    let empty = read(&fixture);
    let restored = prepared
        .restore(empty.clone(), &CancellationToken::default())
        .unwrap();
    assert_eq!(restored.unresolved().len(), 1);
    assert_eq!(restored.unresolved()[0].id(), 10000);
    assert_eq!(
        restored.unresolved()[0].reason(),
        UnresolvedCatalogueChoiceReasonV1::NotObserved
    );
    let mut next = workspace(empty);
    next.restore_observed_pins(old.intent()).unwrap();
    restored.install(&mut next).unwrap();
    assert!(next.selected_ids().is_empty());
    assert!(!next.pins().contains(&10000));
    assert!(
        next.prepare_saved_intent(old.intent(), "original").is_err(),
        "Missing transient intent cannot be silently autosaved/forgotten"
    );
    assert!(next
        .unresolved_choices_notice()
        .contains("Unresolved local choices: 1"));
    let recovered = next
        .snapshot_transient_choices()
        .unwrap()
        .prepare(&CancellationToken::default())
        .unwrap()
        .restore(read(&fixture), &CancellationToken::default())
        .unwrap();
    assert!(recovered.unresolved().is_empty());
    let mut final_workspace = workspace(recovered.source.clone());
    recovered.install(&mut final_workspace).unwrap();
    assert!(final_workspace.selected_ids().contains(&10000));
    assert!(final_workspace.pins().contains(&10000));
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_transient_unknown_and_changed_nodes_never_alias_the_same_numeric_id() {
    for mode in 0..3 {
        let mut old = support::row(0);
        let mut fresh = old.clone();
        match mode {
            0 => old["node_id"] = json!(null),
            1 => fresh["node_id"] = json!(null),
            _ => fresh["node_id"] = json!("R_RECREATED_10000"),
        }
        let mut steps = support::collection(&[old], 20, 42, 42);
        steps.extend(support::collection(&[fresh], 20, 42, 42));
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let original = support::load(
            &fixture,
            &crate::providers::gh::RepositoryCatalogueLimits::default(),
        );
        let mut original_workspace = workspace(original.source().clone());
        select(&mut original_workspace, 0, true);
        let prepared = original_workspace
            .snapshot_transient_choices()
            .unwrap()
            .prepare(&CancellationToken::default())
            .unwrap();
        let current = read(&fixture);
        let restored = prepared
            .restore(current.clone(), &CancellationToken::default())
            .unwrap();
        assert_eq!(restored.unresolved().len(), 1);
        assert_eq!(
            restored.unresolved()[0].reason(),
            [
                UnresolvedCatalogueChoiceReasonV1::OriginalNodeUnavailable,
                UnresolvedCatalogueChoiceReasonV1::CurrentNodeUnavailable,
                UnresolvedCatalogueChoiceReasonV1::NodeChanged
            ][mode]
        );
        let mut next = workspace(current);
        restored.install(&mut next).unwrap();
        assert!(next.selected_ids().is_empty());
        assert!(next.pin_changes.is_empty());
        assert_eq!(next.unresolved_choice_count(), 1);
        support::exact(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_read_limits_transient_source_uid43_and_equaltext_different_read_arc_refuse() {
    let rows = [support::row(0)];
    let mut steps = support::collection(&rows, 20, 42, 42);
    steps.extend(support::collection(&rows, 20, 43, 43));
    steps.extend(support::collection(&rows, 20, 42, 42));
    steps.extend(support::collection(&rows, 20, 42, 42));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let old = support::load(
        &fixture,
        &crate::providers::gh::RepositoryCatalogueLimits::default(),
    );
    let mut original = workspace(old.source().clone());
    select(&mut original, 0, true);
    let prepared = original
        .snapshot_transient_choices()
        .unwrap()
        .prepare(&CancellationToken::default())
        .unwrap();
    assert!(prepared
        .restore(read(&fixture), &CancellationToken::default())
        .is_err());
    let fresh = read(&fixture);
    let restored = prepared
        .restore(fresh, &CancellationToken::default())
        .unwrap();
    let mut equaltext = workspace(read(&fixture));
    assert!(restored.install(&mut equaltext).is_err());
    assert!(equaltext.selected_ids().is_empty());
    assert!(equaltext.pins().is_empty());
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_transient_raw128k_budget_and_cancel_refuse_before_node_copies_or_api() {
    let rows = (0..150)
        .map(|index| {
            let mut row = support::row(index);
            row["node_id"] = json!(format!("R_{index:04}_{}", "N".repeat(1000)));
            row
        })
        .collect::<Vec<_>>();
    let steps = support::collection(&rows, 20, 42, 42);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = support::load(
        &fixture,
        &crate::providers::gh::RepositoryCatalogueLimits::default(),
    );
    let mut current = workspace(original.source().clone());
    current.selected = (10000..10150).collect();
    let snapshot = current.snapshot_transient_choices().unwrap();
    assert!(snapshot
        .prepare(&CancellationToken::default())
        .unwrap_err()
        .contains("128KiB"));
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(snapshot.prepare(&cancel).is_err());
    assert_eq!(current.selected_ids().len(), 150);
    assert_eq!(support::generation(&fixture, &original), 1);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_transient_union_precopy_cap_and_explicit_discard_preserve_durable_pins()
{
    let rows = [support::row(0)];
    let mut steps = support::collection(&rows, 20, 42, 42);
    steps.extend(support::collection(&[], 20, 42, 42));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = support::load(
        &fixture,
        &crate::providers::gh::RepositoryCatalogueLimits::default(),
    );
    let mut current = workspace(original.source().clone());
    current.selected = (1..=4097).collect();
    assert!(current.snapshot_transient_choices().is_err());
    current.selected.clear();
    select(&mut current, 0, true);
    let prepared = current
        .snapshot_transient_choices()
        .unwrap()
        .prepare(&CancellationToken::default())
        .unwrap();
    let missing = read(&fixture);
    let restored = prepared
        .restore(missing.clone(), &CancellationToken::default())
        .unwrap();
    let mut durable = original.intent().clone();
    durable.pin(original.source(), 10000, true).unwrap();
    let before = durable.to_value().unwrap();
    let mut next = workspace(missing);
    next.restore_observed_pins(&durable).unwrap();
    restored.install(&mut next).unwrap();
    next.discard_unresolved_choices();
    assert_eq!(next.unresolved_choice_count(), 0);
    assert!(next.pins().contains(&10000));
    assert_eq!(durable.to_value().unwrap(), before);
    assert_eq!(
        next.prepare_saved_intent(&durable, "unchanged")
            .unwrap()
            .pins(),
        durable.pins()
    );
    assert_eq!(support::generation(&fixture, &original), 1);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_transient_conflicting_new_node_intent_requires_deliberate_discard() {
    let old = [support::row(0)];
    let mut changed = old.clone();
    changed[0]["node_id"] = json!("NEW_NODE_SAME_ID");
    let mut steps = support::collection(&old, 20, 42, 42);
    steps.extend(support::collection(&changed, 20, 42, 42));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = support::load(
        &fixture,
        &crate::providers::gh::RepositoryCatalogueLimits::default(),
    );
    let mut previous = workspace(original.source().clone());
    select(&mut previous, 0, true);
    let prepared = previous
        .snapshot_transient_choices()
        .unwrap()
        .prepare(&CancellationToken::default())
        .unwrap();
    let fresh = read(&fixture);
    let restored = prepared
        .restore(fresh.clone(), &CancellationToken::default())
        .unwrap();
    let mut next = workspace(fresh);
    restored.install(&mut next).unwrap();
    select(&mut next, 0, false);
    let snapshot = next.snapshot_transient_choices().unwrap();
    assert!(snapshot
        .prepare(&CancellationToken::default())
        .unwrap_err()
        .contains("conflicts"));
    next.discard_unresolved_choices();
    assert!(next
        .snapshot_transient_choices()
        .unwrap()
        .prepare(&CancellationToken::default())
        .is_ok());
    support::exact(&fixture, &steps);
}
