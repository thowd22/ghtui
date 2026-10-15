use super::*;
use crate::{
    git::Comparison,
    local_commit_actor_origin_v1_test_support::Real,
    original_diff_sides_v1_test_support as fixture,
    repository_diff_sides_worker_v1::{OriginalDiffWorkV1, OriginalDiffWorkerOwnerV1},
};
fn loaded(empty: bool) -> (Real, Workspace, String, String) {
    let real = Real::new("sha256");
    let old = fixture::commit(
        &real,
        &fixture::tree(&real, &[(b"z-small.rs", b"old\n", "100644")]),
        None,
        "Old",
    );
    let large = vec![b'x'; 800_000];
    let new_tree = if empty {
        fixture::tree(&real, &[(b"z-small.rs", b"old\n", "100644")])
    } else {
        fixture::tree(
            &real,
            &[
                (b"a-large.rs", &large, "100644"),
                (b"z-small.rs", b"small actual target\n", "100644"),
            ],
        )
    };
    let new = fixture::commit(&real, &new_tree, Some(&old), "New");
    let workspace = fixture::workspace(
        &real,
        Comparison::Snapshots {
            base: old.clone(),
            head: new.clone(),
        },
    );
    assert!(workspace.original_comparison_v1.is_none());
    (real, workspace, old, new)
}
fn owner(workspace: &Workspace, state: &mut NativeDiffStateV1) -> Arc<OriginalDiffWorkerOwnerV1> {
    let request = state.request().unwrap();
    OriginalDiffWorkerOwnerV1::begin(
        workspace,
        None,
        Arc::new(OriginalDiffWorkV1::Native(request)),
        Default::default(),
    )
    .unwrap()
}
#[test]
fn native_diff_metadata_genuine_first_factory_oversize_retains_sealed_source_no_rows_then_other_file_zero_git(
) {
    let (real, workspace, old, new) = loaded(false);
    let mut state = NativeDiffStateV1::open(&workspace, 118).unwrap();
    let first = owner(&workspace, &mut state);
    let before = real.snapshot();
    real.clear_trace();
    let prepared = first.prepare().unwrap();
    assert!(prepared.matches(&first));
    let accounting = first.accounting().unwrap();
    assert!(accounting.sealed());
    assert_eq!(
        (
            accounting.children_started(),
            accounting.in_flight_requests()
        ),
        (8, 0)
    );
    prepared.publish_native_v1(&workspace, &mut state).unwrap();
    let source = state.source().unwrap().clone();
    assert_eq!(source.intent().content_base(), old);
    assert_eq!(source.intent().head(), new);
    assert_eq!(source.members().len(), 2);
    assert!(state
        .metadata_note()
        .unwrap()
        .contains("original catalogue retained"));
    assert!(state.frame().is_none());
    assert!(state.current_frame().is_none());
    assert!(state.range().is_none());
    assert!(state.select_hunk(0).is_err());
    assert!(workspace.original_comparison_v1.is_none());
    real.clear_trace();
    state.file = 1;
    let next = owner(&workspace, &mut state);
    next.prepare()
        .unwrap()
        .publish_native_v1(&workspace, &mut state)
        .unwrap();
    assert_eq!(next.accounting().unwrap().children_started(), 0);
    assert!(Arc::ptr_eq(state.source().unwrap(), &source));
    assert!(state
        .current_frame()
        .unwrap()
        .rows()
        .iter()
        .any(|row| row.left().text().contains("small actual target")));
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
#[test]
fn native_diff_metadata_actual_complete_no_change_has_no_fake_member_key_range_or_staging_authority(
) {
    let (real, workspace, old, new) = loaded(true);
    let mut state = NativeDiffStateV1::open(&workspace, 118).unwrap();
    let operation = owner(&workspace, &mut state);
    let before = real.snapshot();
    real.clear_trace();
    operation
        .prepare()
        .unwrap()
        .publish_native_v1(&workspace, &mut state)
        .unwrap();
    assert!(operation.accounting().unwrap().sealed());
    assert_eq!(operation.accounting().unwrap().children_started(), 8);
    let source = state.source().unwrap();
    assert!(source.complete());
    assert_eq!(source.observed_members(), 0);
    assert!(source.members().is_empty());
    assert_eq!(source.intent().content_base(), old);
    assert_eq!(source.intent().head(), new);
    assert!(state.current_frame().is_none() && state.frame().is_none() && state.range().is_none());
    assert!(state.select_hunk(0).is_err());
    assert!(state
        .status
        .contains("No changed files in this complete original comparison"));
    assert_eq!(real.snapshot(), before);
}
#[test]
fn native_diff_metadata_late_factory_result_cannot_install_source_after_new_query_purpose_or_close()
{
    let (real, workspace, _, _) = loaded(false);
    let before = real.snapshot();
    for changed in 0..3 {
        let mut state = NativeDiffStateV1::open(&workspace, 118).unwrap();
        let first = owner(&workspace, &mut state);
        let prepared = first.prepare().unwrap();
        match changed {
            0 => {
                state.query.insert("new").unwrap();
                state.request().unwrap();
            }
            1 => {
                state.width = 38;
                state.request().unwrap();
            }
            _ => state = NativeDiffStateV1::open(&workspace, 118).unwrap(),
        };
        assert!(prepared.publish_native_v1(&workspace, &mut state).is_err());
        assert!(state.source().is_none() && state.frame().is_none() && state.range().is_none());
    }
    assert_eq!(real.snapshot(), before);
}
#[test]
fn native_diff_metadata_invalid_query_retains_only_previous_labelled_preview_and_admitted_range() {
    let (real, workspace, _, _) = loaded(false);
    let mut state = NativeDiffStateV1::open(&workspace, 118).unwrap();
    let first = owner(&workspace, &mut state);
    first
        .prepare()
        .unwrap()
        .publish_native_v1(&workspace, &mut state)
        .unwrap();
    state.file = 1;
    let next = owner(&workspace, &mut state);
    next.prepare()
        .unwrap()
        .publish_native_v1(&workspace, &mut state)
        .unwrap();
    let frame = state.current_frame().unwrap().clone();
    let ticket = (0..frame.rows().len())
        .find_map(|row| {
            frame
                .ticket(row, false)
                .filter(|ticket| ticket.key().canonical().kind == crate::diff::LineKind::Added)
        })
        .unwrap();
    state.select(ticket.clone(), false).unwrap();
    let select = owner(&workspace, &mut state);
    select
        .prepare()
        .unwrap()
        .publish_native_v1(&workspace, &mut state)
        .unwrap();
    let admitted = state.range().unwrap().clone();
    let preview = state.frame().unwrap().clone();
    state.regex = true;
    state.query.insert("[").unwrap();
    real.clear_trace();
    let bad = owner(&workspace, &mut state);
    bad.prepare()
        .unwrap()
        .publish_native_v1(&workspace, &mut state)
        .unwrap();
    assert!(bad.accounting().unwrap().sealed());
    assert_eq!(bad.accounting().unwrap().children_started(), 0);
    assert!(state.current_frame().is_none());
    assert!(Arc::ptr_eq(state.frame().unwrap(), &preview));
    assert!(Arc::ptr_eq(state.range().unwrap(), &admitted));
    assert!(state.select(ticket, false).is_err());
    assert!(state.status.starts_with("Error:"));
    assert!(real.children().is_empty());
}
