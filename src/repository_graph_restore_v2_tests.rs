use super::*;
use crate::local_commit_actor_origin_v1_test_support::{git, Real};
use crate::repository_graph_v1::fixture_v2::{all, first, more, ordinary};

#[test]
fn commit_graph_v2_position_changes_keep_source_more_changes_it_and_requested_pose_is_immutable() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 240);
    let state = first(&real, &tip);
    let snapshot = state.snapshot_v2();
    let selected = state.member_at_v2(100).unwrap().id.clone();
    let original = state
        .position_v2(&selected, GraphViewV1::Timeline, 0, 16, None)
        .unwrap();
    let changed = state
        .position_v2(&selected, GraphViewV1::Details, 0, 32, None)
        .unwrap();
    assert_ne!(original, changed);
    assert!(Arc::ptr_eq(&snapshot, &state.snapshot_v2()));
    assert_eq!(original.horizontal, 16);
    assert_eq!(original.view, GraphViewV1::Timeline);
    let next = more(&state);
    assert_ne!(snapshot.source_key(), next.snapshot_v2().source_key());
    let mut fake = original.clone();
    fake.member.node = 101;
    assert!(snapshot.validate_position(&fake).is_err());
    fake = original.clone();
    fake.selected = tip;
    assert!(snapshot.validate_position(&fake).is_err());
    fake = original;
    fake.projection_start = 200;
    assert!(snapshot.validate_position(&fake).is_err());
}
#[test]
fn commit_graph_v2_unique_actual_arc_charge_pending_drop_and64_source_refusal_are_atomic() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 10_000);
    let budget = GraphRetentionV2::new();
    let empty = budget.accounting().unwrap();
    let mut state = first(&real, &tip);
    let mut leases = vec![budget.admit(state.snapshot_v2()).unwrap()];
    let initial = budget.accounting().unwrap();
    let same = budget.admit(state.snapshot_v2()).unwrap();
    assert!(Arc::ptr_eq(&leases[0], &same));
    assert_eq!(budget.accounting().unwrap(), initial);
    drop(same);
    while state.last_page().has_more() {
        state = more(&state);
        leases.push(budget.admit(state.snapshot_v2()).unwrap());
    }
    assert_eq!(state.loaded(), 10_000);
    let loaded = budget.accounting().unwrap();
    assert_eq!(loaded.sources, 50);
    assert!(loaded.bytes <= GRAPH_VISIT_BYTES_V2);
    let pending = leases[24].clone();
    let pending_bytes = budget.accounting().unwrap();
    leases.remove(24);
    assert_eq!(budget.accounting().unwrap(), pending_bytes);
    drop(pending);
    assert_eq!(budget.accounting().unwrap().sources, 49);
    // Distinct genuine rereads of the same textual IDs are NOT actualArc dedup.
    while leases.len() < 64 {
        let distinct = first(&real, state.walk_tip());
        leases.push(budget.admit(distinct.snapshot_v2()).unwrap());
    }
    let capped = budget.accounting().unwrap();
    let extra = first(&real, state.walk_tip());
    assert!(budget
        .admit(extra.snapshot_v2())
        .unwrap_err()
        .contains("limit64"));
    assert_eq!(budget.accounting().unwrap(), capped);
    leases.clear();
    assert_eq!(budget.accounting().unwrap(), empty);
}
#[test]
fn commit_graph_v2_full10000_back_uses_fresh_exact_object_not_page1_reread_or_old_pool() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 10_000);
    let baseline = real.snapshot();
    let state = all(&real, &tip);
    let id = state.member_at_v2(423).unwrap().id.clone();
    let position = state
        .position_v2(&id, GraphViewV1::Parents, 400, 24, None)
        .unwrap();
    let budget = GraphRetentionV2::new();
    let lease = budget.admit(state.snapshot_v2()).unwrap();
    drop(state);
    real.clear_trace();
    let operation =
        LocalGitNavigationReadV1::begin(lease.snapshot.origin(), Default::default()).unwrap();
    let restored = lease.snapshot.restore(&position, &operation).unwrap();
    operation.seal().unwrap();
    assert_eq!(restored.state.loaded(), 10_000);
    assert_eq!(restored.state.walk_tip(), tip);
    assert_eq!(restored.position, position);
    assert!(restored
        .presentation
        .rows()
        .iter()
        .any(|r| r.label.starts_with("Parent 1:")));
    let children = real.children();
    assert!(!children.is_empty());
    assert!(!children
        .iter()
        .any(|row| row["argv"].as_array().is_some_and(|argv| argv
            .iter()
            .any(|v| v == "log" || v == "rev-list" || v == "show-ref"))));
    assert!(
        children.len() <= 8,
        "Exact original member is bounded, not50 page reconstruction"
    );
    assert!(lease
        .snapshot
        .restore(&position, &operation)
        .unwrap_err()
        .contains("completed"));
    assert_eq!(real.snapshot(), baseline);
}
#[test]
fn commit_graph_v2_restore_refuses_shallow_changed_source_and_precanceled_request_without_projection(
) {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 230);
    let state = all(&real, &tip);
    let snapshot = state.snapshot_v2();
    let position = state
        .position_v2(&tip, GraphViewV1::Timeline, 0, 0, None)
        .unwrap();
    let token = crate::providers::runner::CancellationToken::default();
    token.cancel();
    real.clear_trace();
    assert!(LocalGitNavigationReadV1::begin(snapshot.origin(), token).is_err());
    assert!(real.children().is_empty());
    std::fs::write(real.root.join(".git/shallow"), format!("{tip}\n")).unwrap();
    let operation = LocalGitNavigationReadV1::begin(snapshot.origin(), Default::default()).unwrap();
    assert!(snapshot.restore(&position, &operation).is_err());
    assert!(!real.children().iter().any(|r| r["argv"]
        .as_array()
        .is_some_and(|argv| argv.iter().any(|v| v == "--batch" || v == "--batch-check"))));
    drop(operation);
    std::fs::remove_file(real.root.join(".git/shallow")).unwrap();
    let gitdir = real.root.join(".git");
    std::fs::rename(&gitdir, real.root.join(".retired-git")).unwrap();
    git(&real.root, &["init", "-q"], None);
    let operation = LocalGitNavigationReadV1::begin(snapshot.origin(), Default::default()).unwrap();
    assert!(snapshot.restore(&position, &operation).is_err());
}
