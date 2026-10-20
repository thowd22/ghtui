//! SOURCE-ONLY declarations. Every snapshot comes from actual installed-Git
//! factories; no forged page/observed DTO or censored10000 fixture.
use super::*;
use crate::{
    local_commit_actor_origin_v1_test_support::{git, text, Real},
    providers::runner::CancellationToken,
    repository_graph_v1::{
        fixture_v2::{all, first, ordinary},
        GraphViewV1,
    },
    workspace::{RepositoryMode, RowAction},
};

fn loaded(real: &Real, count: usize) -> Workspace {
    let tip = ordinary(real, count);
    let state = all(real, &tip);
    let position = state
        .position_v2(state.walk_tip(), GraphViewV1::Timeline, 0, 0, None)
        .unwrap();
    let operation = LocalGitNavigationReadV1::begin(state.origin(), Default::default()).unwrap();
    let workspace =
        Workspace::restore_graph_snapshot_v2(&state.snapshot_v2(), &position, None, &operation)
            .unwrap();
    operation.seal().unwrap();
    assert_eq!(workspace.graph_state_v1.as_ref().unwrap().loaded(), count);
    workspace
}
fn no_history_walk(real: &Real) {
    assert!(!real
        .children()
        .iter()
        .any(|child| child["argv"].as_array().is_some_and(|argv| argv
            .iter()
            .any(|arg| arg == "log" || arg == "rev-list" || arg == "show-ref"))));
}

#[test]
fn commit_graph_v3_every_distinct_actual_visit_allocation_is_charged_until_last_drop() {
    let real = Real::new("sha1");
    let workspace = loaded(&real, 230);
    let mut registry = OriginalSavedVisitsV1::default();
    let first = registry.capture_graph_v2(&workspace).unwrap();
    registry.remember_checked_v2(first.clone()).unwrap();
    let lease = first.graph_lease_v2().unwrap().clone();
    let before = registry.graph_retention_accounting_v2().unwrap().unwrap();
    let second = OriginalSavedVisitV1::from_graph_lease_v3(&workspace, lease).unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
    let two = registry.graph_retention_accounting_v2().unwrap().unwrap();
    assert_eq!(two.sources, before.sources);
    assert!(two.bytes > before.bytes);
    let held = second.clone();
    drop(second);
    assert_eq!(
        registry.graph_retention_accounting_v2().unwrap().unwrap(),
        two
    );
    drop(held);
    assert_eq!(
        registry.graph_retention_accounting_v2().unwrap().unwrap(),
        before
    );
}

#[test]
fn commit_graph_v3_refresh_actual_full10000_same_pages_position_historical_names_and_dirty_status()
{
    let real = Real::new("sha1");
    let mut workspace = loaded(&real, 10_000);
    workspace
        .graph_window_action_v2(crate::repository_graph_v1::GraphWindowActionV2::Older)
        .unwrap();
    workspace
        .open(RowAction::Graph(
            crate::repository_graph_v1::GraphActionV1::Parents,
        ))
        .unwrap();
    workspace.graph_horizontal_v1 = 24;
    let position = workspace.graph_position_v2().unwrap();
    let snapshot = workspace.graph_state_v1.as_ref().unwrap().snapshot_v2();
    let observed = workspace
        .graph_state_v1
        .as_ref()
        .unwrap()
        .selected_commit(workspace.graph_state_v1.as_ref().unwrap().walk_tip())
        .unwrap()
        .refs
        .clone();
    // Current names may move, but snapshot names stay honestly observed-at-load.
    git(
        &real.root,
        &["update-ref", "refs/heads/graph-v2", &real.head],
        None,
    );
    git(
        &real.root,
        &["tag", "later-name", snapshot.walk_tip()],
        None,
    );
    assert_eq!(
        text(&real.root, &["rev-parse", "refs/heads/graph-v2"]),
        real.head
    );
    std::fs::write(
        real.root.join("tracked"),
        b"fresh dirty status after graph load\n",
    )
    .unwrap();
    let baseline = real.snapshot();
    real.clear_trace();
    let owner = crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1::begin(
        &workspace,
        Default::default(),
    )
    .unwrap();
    let refreshed = workspace
        .refresh_original_graph_v3(owner.operation())
        .unwrap();
    owner.operation().seal().unwrap();
    assert_eq!(refreshed.graph_state_v1.as_ref().unwrap().loaded(), 10_000);
    assert!(snapshot.same_source(&refreshed.graph_state_v1.as_ref().unwrap().snapshot_v2()));
    assert_eq!(refreshed.graph_position_v2().unwrap(), position);
    assert_eq!(
        refreshed
            .graph_state_v1
            .as_ref()
            .unwrap()
            .selected_commit(snapshot.walk_tip())
            .unwrap()
            .refs,
        observed
    );
    assert!(
        !refreshed.status.entries.is_empty(),
        "Working status is freshly obtained, not replayed"
    );
    assert_eq!(
        real.children().len(),
        10,
        "Four current status/shallow/sparse/override reads plus exact member/epochs, not50page rebuilding"
    );
    no_history_walk(&real);
    assert!(owner.operation().check().unwrap_err().contains("completed"));
    assert_eq!(real.snapshot(), baseline);
}

#[test]
fn commit_graph_v3_explicit_reread_full10000_new_first200_keeps_exact_old_back_and_actual_ledger() {
    let real = Real::new("sha1");
    let mut workspace = loaded(&real, 10_000);
    workspace
        .graph_window_action_v2(crate::repository_graph_v1::GraphWindowActionV2::Older)
        .unwrap();
    workspace.graph_horizontal_v1 = 16;
    let old_position = workspace.graph_position_v2().unwrap();
    let old_snapshot = workspace.graph_state_v1.as_ref().unwrap().snapshot_v2();
    let mut registry = OriginalSavedVisitsV1::default();
    real.clear_trace();
    let preview = GraphRereadPreviewV3::capture(&workspace, &mut registry).unwrap();
    assert!(preview.matches_workspace(&workspace));
    assert!(
        real.children().is_empty(),
        "Preview and retention preflight are pure"
    );
    let before = preview.retention.accounting().unwrap();
    let baseline = real.snapshot();
    real.clear_trace();
    let operation =
        LocalGitNavigationReadV1::begin(preview.original.origin(), Default::default()).unwrap();
    let result = preview.execute(&operation).unwrap();
    let deadline = operation.deadline();
    let closed = operation.seal().unwrap();
    assert_eq!(closed.children_started(), 20);
    assert_eq!(closed.max_requests(), 128);
    assert_eq!(operation.deadline(), deadline);
    let children_after_closure = real.children().len();
    assert!(preview.execute(&operation).is_err());
    assert_eq!(
        real.children().len(),
        children_after_closure,
        "Completed original window cannot be revived for another reread"
    );
    let fresh = result.workspace.graph_state_v1.as_ref().unwrap();
    assert_eq!(fresh.loaded(), 200);
    assert_eq!(fresh.walk_tip(), old_snapshot.walk_tip());
    assert_eq!(result.workspace.revision, old_snapshot.walk_tip());
    assert_eq!(result.workspace.graph_projection_start_v2, 0);
    assert_eq!(result.workspace.graph_horizontal_v1, 0);
    assert_eq!(result.workspace.graph_view_v1, GraphViewV1::Timeline);
    assert!(!fresh.snapshot_v2().same_source(&old_snapshot));
    assert_eq!(
        preview.retention.accounting().unwrap().sources,
        before.sources + 1
    );
    assert_eq!(
        real.children().len(),
        20,
        "Bounded10 snapshot recheck+9 first-page factory+1 final epoch; one original request"
    );
    assert_eq!(
        real.children()
            .iter()
            .filter(|child| child["argv"]
                .as_array()
                .is_some_and(|argv| argv.iter().any(|arg| arg == "log")))
            .count(),
        1
    );
    registry
        .accept_graph_reread_v3(&result.workspace, result.visit)
        .unwrap();
    let old = registry.graph_visit_v2(&workspace).unwrap();
    real.clear_trace();
    let back_request = old.graph_request_position_v2(&old_position).unwrap();
    let back_op = LocalGitNavigationReadV1::begin(old.origin(), Default::default()).unwrap();
    let back = old
        .restore_graph_position_v2(&back_request, &back_op)
        .unwrap();
    back_op.seal().unwrap();
    assert_eq!(back.graph_state_v1.as_ref().unwrap().loaded(), 10_000);
    assert_eq!(back.graph_position_v2().unwrap(), old_position);
    assert!(old_snapshot.same_source(&back.graph_state_v1.as_ref().unwrap().snapshot_v2()));
    no_history_walk(&real);
    assert_eq!(real.snapshot(), baseline);
}

#[test]
fn commit_graph_v3_cap64_preflight_before_child_and_tentative_source_drop_is_atomic() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 230);
    let mut registry = OriginalSavedVisitsV1::default();
    let mut current = None;
    for _ in 0..64 {
        let state = first(&real, &tip);
        let position = state
            .position_v2(&tip, GraphViewV1::Timeline, 0, 0, None)
            .unwrap();
        let operation =
            LocalGitNavigationReadV1::begin(state.origin(), Default::default()).unwrap();
        let workspace =
            Workspace::restore_graph_snapshot_v2(&state.snapshot_v2(), &position, None, &operation)
                .unwrap();
        operation.seal().unwrap();
        let visit = registry.capture_graph_v2(&workspace).unwrap();
        registry.remember_checked_v2(visit).unwrap();
        current = Some(workspace);
    }
    let accounting = registry.graph_retention_accounting_v2().unwrap().unwrap();
    assert_eq!(accounting.sources, 64);
    real.clear_trace();
    assert!(
        GraphRereadPreviewV3::capture(current.as_ref().unwrap(), &mut registry)
            .unwrap_err()
            .contains("64")
    );
    assert!(real.children().is_empty());
    assert_eq!(
        registry.graph_retention_accounting_v2().unwrap().unwrap(),
        accounting
    );
    let mut small = OriginalSavedVisitsV1::default();
    let preview = GraphRereadPreviewV3::capture(current.as_ref().unwrap(), &mut small).unwrap();
    let original = preview.retention.accounting().unwrap();
    let operation =
        LocalGitNavigationReadV1::begin(preview.original.origin(), Default::default()).unwrap();
    let tentative = preview.execute(&operation).unwrap();
    operation.seal().unwrap();
    assert_eq!(preview.retention.accounting().unwrap().sources, 2);
    drop(tentative);
    assert_eq!(
        preview.retention.accounting().unwrap(),
        original,
        "Unpublished actual lease releases exactly, no registry rollback fiction"
    );
}

#[test]
fn commit_graph_v3_actual_cumulative16mib_refusal_does_not_reset_existing_ledger() {
    let real = Real::new("sha1");
    let blob = text(&real.root, &["rev-parse", "HEAD:tracked"]);
    let mut input = Vec::new();
    let subject = "s".repeat(4000);
    for number in 1..=200 {
        let message = format!("{subject}\n");
        input.extend_from_slice(format!("commit refs/heads/large-graph-v3\nmark :{number}\nauthor Writer <writer@example.invalid> {} +0000\ncommitter Recorder <recorder@example.invalid> {} +0000\ndata {}\n{message}",1_700_000_000+number,1_700_000_000+number,message.len()).as_bytes());
        if number > 1 {
            input.extend_from_slice(format!("from :{}\n", number - 1).as_bytes());
        } else {
            input.extend_from_slice(format!("deleteall\nM 100644 {blob} tracked\n").as_bytes());
        }
        input.push(b'\n');
    }
    git(&real.root, &["fast-import", "--quiet"], Some(&input));
    let tip = text(&real.root, &["rev-parse", "refs/heads/large-graph-v3"]);
    let budget = GraphRetentionV2::new();
    let mut leases = Vec::new();
    loop {
        let state = first(&real, &tip);
        let before = budget.accounting().unwrap();
        match budget.admit(state.snapshot_v2()) {
            Ok(lease) => {
                leases.push(lease);
                assert!(leases.len() < 64);
            }
            Err(reason) => {
                assert!(
                    reason.contains("16MiB"),
                    "Actual bytes must cap before64 sources: {reason}"
                );
                assert_eq!(budget.accounting().unwrap(), before);
                assert!(
                    before.bytes <= 16 * 1024 * 1024 && before.sources > 1 && before.sources < 64
                );
                assert!(
                    !budget.owns_lease_v3(
                        &GraphRetentionV2::new()
                            .admit(first(&real, &tip).snapshot_v2())
                            .unwrap()
                    ),
                    "Another budget cannot substitute this one"
                );
                break;
            }
        }
    }
}

#[test]
fn commit_graph_v3_canceled_prequeue_original_window_has_zero_children_and_no_retention_change() {
    let real = Real::new("sha1");
    let workspace = loaded(&real, 230);
    let mut registry = OriginalSavedVisitsV1::default();
    let preview = GraphRereadPreviewV3::capture(&workspace, &mut registry).unwrap();
    let accounting = preview.retention.accounting().unwrap();
    let cancel = CancellationToken::default();
    let operation =
        LocalGitNavigationReadV1::begin(preview.original.origin(), cancel.clone()).unwrap();
    cancel.cancel();
    real.clear_trace();
    assert!(preview.execute(&operation).is_err());
    assert!(operation.seal().is_err());
    assert!(real.children().is_empty());
    assert_eq!(preview.retention.accounting().unwrap(), accounting);
    assert!(preview.matches_workspace(&workspace));
}

#[test]
fn commit_graph_v3_shallow_source_changed_refuses_before_reread_walk_not_old_pages_as_current() {
    let real = Real::new("sha1");
    let workspace = loaded(&real, 230);
    let mut registry = OriginalSavedVisitsV1::default();
    let preview = GraphRereadPreviewV3::capture(&workspace, &mut registry).unwrap();
    let accounting = preview.retention.accounting().unwrap();
    std::fs::write(
        real.root.join(".git/shallow"),
        format!(
            "{}\n",
            workspace.graph_state_v1.as_ref().unwrap().walk_tip()
        ),
    )
    .unwrap();
    let operation =
        LocalGitNavigationReadV1::begin(preview.original.origin(), Default::default()).unwrap();
    real.clear_trace();
    assert!(preview.execute(&operation).is_err());
    let _ = operation.seal();
    no_history_walk(&real);
    assert_eq!(preview.retention.accounting().unwrap(), accounting);
    assert_eq!(workspace.graph_state_v1.as_ref().unwrap().loaded(), 230);
}

#[test]
fn commit_graph_v3_pending_position_is_immutable_and_equal_text_new_source_cannot_own_preview() {
    let real = Real::new("sha1");
    let mut workspace = loaded(&real, 230);
    let mut registry = OriginalSavedVisitsV1::default();
    let preview = GraphRereadPreviewV3::capture(&workspace, &mut registry).unwrap();
    let position = preview.position.position().clone();
    workspace
        .open(RowAction::Graph(
            crate::repository_graph_v1::GraphActionV1::Details,
        ))
        .unwrap();
    assert!(!preview.matches_workspace(&workspace));
    assert_eq!(*preview.position.position(), position);
    let other = Workspace::load(
        &real.root,
        RepositoryMode::Graph,
        preview
            .original
            .graph_lease_v2()
            .unwrap()
            .snapshot()
            .walk_tip(),
        None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(other.revision, position.selected);
    assert!(!preview.matches_workspace(&other));
    assert!(!preview
        .original
        .graph_lease_v2()
        .unwrap()
        .snapshot()
        .same_source(&other.graph_state_v1.as_ref().unwrap().snapshot_v2()));
    let foreign_owner = crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1::begin(
        &other,
        Default::default(),
    )
    .unwrap();
    real.clear_trace();
    assert!(crate::repository_graph_reread_native_v3::work(&preview, &foreign_owner).is_err());
    assert!(
        real.children().is_empty(),
        "A foreign genuine request cannot read this original capsule"
    );
}

#[test]
fn commit_graph_v3_actual_ordered_parent_switch_invalidates_same_arc_source_owner_before_any_child()
{
    let real = Real::new("sha1");
    let tree = text(&real.root, &["rev-parse", "HEAD^{tree}"]);
    let parents = (0..2)
        .map(|number| {
            String::from_utf8(git(
                &real.root,
                &[
                    "commit-tree",
                    &tree,
                    "-p",
                    &real.original,
                    "-m",
                    &format!("Genuine ordered parent {number}"),
                ],
                None,
            ))
            .unwrap()
            .trim()
            .to_owned()
        })
        .collect::<Vec<_>>();
    let merge = String::from_utf8(git(
        &real.root,
        &[
            "commit-tree",
            &tree,
            "-p",
            &parents[0],
            "-p",
            &parents[1],
            "-m",
            "Genuine two-parent V3",
        ],
        None,
    ))
    .unwrap()
    .trim()
    .to_owned();
    let baseline = real.snapshot();
    let mut workspace = Workspace::load(&real.root, RepositoryMode::Graph, &merge, None)
        .unwrap()
        .unwrap();
    let operation = LocalGitNavigationReadV1::begin(
        workspace.actor_origin.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    workspace
        .open_original_with_operation_v1(
            RowAction::Graph(crate::repository_graph_v1::GraphActionV1::CompareParent(0)),
            &operation,
        )
        .unwrap();
    operation.seal().unwrap();
    let mut registry = OriginalSavedVisitsV1::default();
    let preview = GraphRereadPreviewV3::capture(&workspace, &mut registry).unwrap();
    let owner = crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1::begin(
        &workspace,
        Default::default(),
    )
    .unwrap();
    let state = workspace.graph_state_v1.as_ref().unwrap().clone();
    let display = workspace.graph_display_v1.as_ref().unwrap().clone();
    let second = LocalGitNavigationReadV1::begin(
        workspace.actor_origin.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    workspace
        .open_original_with_operation_v1(
            RowAction::Graph(crate::repository_graph_v1::GraphActionV1::CompareParent(1)),
            &second,
        )
        .unwrap();
    second.seal().unwrap();
    assert!(Arc::ptr_eq(
        workspace.graph_state_v1.as_ref().unwrap(),
        &state
    ));
    assert!(Arc::ptr_eq(
        workspace.graph_display_v1.as_ref().unwrap(),
        &display
    ));
    assert_eq!(workspace.graph_view_v1, GraphViewV1::Comparison);
    assert_eq!(
        workspace.graph_position_v2().unwrap().comparison_parent,
        Some(1)
    );
    assert!(!owner.can_admit(&workspace, true));
    assert!(!owner.can_admit(&workspace, false));
    assert!(!preview.matches_workspace(&workspace));
    let new_owner = crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1::begin(
        &workspace,
        Default::default(),
    )
    .unwrap();
    real.clear_trace();
    assert!(crate::repository_graph_reread_native_v3::work(&preview, &new_owner).is_err());
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), baseline);
}
