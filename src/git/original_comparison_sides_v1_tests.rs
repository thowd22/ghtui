use super::*;
use crate::original_diff_sides_v1_test_support as support;
#[test]
fn original_diff_sides_original_expired_clock_no_source_preflight_or_git_child() {
    let (real, a, b, _) = support::fixture("sha1");
    let workspace = support::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let intent = OriginalComparisonIntentV1::capture(&workspace).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    let op = LocalGitNavigationReadV1::begin_under_deadline(
        intent.origin(),
        Default::default(),
        deadline,
    )
    .unwrap();
    while std::time::Instant::now() < op.deadline() {
        std::thread::yield_now();
    }
    real.clear_trace();
    assert!(OriginalComparisonSourceV1::read(intent, &op).is_err());
    assert!(real.children().is_empty());
    assert_eq!(op.accounting().unwrap().children_started(), 0);
}
#[test]
fn original_diff_sides_sha1_proposed_actual_old_c_is_not_requested_a() {
    proposed("sha1");
}
#[test]
fn original_diff_sides_sha256_proposed_actual_old_c_is_not_requested_a() {
    proposed("sha256");
}
fn proposed(format: &str) {
    let (real, a, b, c) = support::fixture(format);
    let workspace = support::workspace(
        &real,
        Comparison::BranchChanges {
            base: a.clone(),
            head: b.clone(),
        },
    );
    let before = real.snapshot();
    real.clear_trace();
    let source = support::source(&workspace);
    assert_eq!(source.intent().requested_base(), Some(a.as_str()));
    assert_eq!(
        source.endpoint(OriginalComparisonSideV1::Old).commit(),
        Some(c.as_str())
    );
    assert_eq!(
        source.endpoint(OriginalComparisonSideV1::New).commit(),
        Some(b.as_str())
    );
    assert_ne!(
        source.intent().requested_base(),
        source.endpoint(OriginalComparisonSideV1::Old).commit()
    );
    assert_eq!(source.head_commit().author.name, "Target Writer");
    assert!(source.complete());
    assert_eq!(source.members().len(), 4);
    assert_eq!(
        real.children().len(),
        9,
        "compare resolves2/merge1/patch1/names1, commit-size1/log3"
    );
    support::assert_no_discovery(&real);
    assert_eq!(real.snapshot(), before);
}
#[test]
fn original_diff_sides_snapshot_question_named_refs_pin_actual_full_endpoints() {
    let (real, a, b, _) = support::fixture("sha1");
    let workspace = support::workspace(
        &real,
        Comparison::Snapshots {
            base: "requested-A".into(),
            head: "target-B".into(),
        },
    );
    let intent = OriginalComparisonIntentV1::capture(&workspace).unwrap();
    assert!(intent.owns(&workspace));
    let source = support::source(&workspace);
    assert_eq!(
        source.endpoint(OriginalComparisonSideV1::Old).commit(),
        Some(a.as_str())
    );
    assert_eq!(
        source.endpoint(OriginalComparisonSideV1::New).commit(),
        Some(b.as_str())
    );
}
#[test]
fn original_diff_sides_actual_raw_root_proof_not_a_shallow_walk_empty_root() {
    let (real, _, _, root) = support::fixture("sha256");
    let workspace = support::workspace(
        &real,
        Comparison::CommitChanges {
            commit: root.clone(),
            parent: 0,
        },
    );
    let source = support::source(&workspace);
    assert!(
        matches!(source.endpoint(OriginalComparisonSideV1::Old),OriginalComparisonEndpointV1::EmptyRoot{introducing_commit,..} if introducing_commit==&root)
    );
    assert!(source
        .members()
        .iter()
        .all(|row| row.path(OriginalComparisonSideV1::Old).is_none()));
    assert!(source.head_commit().parents.is_empty());
}
#[test]
fn original_diff_sides_shallow_raw_parent_order_remains_non_root() {
    let (real, _, b, c) = support::fixture("sha1");
    std::fs::write(real.root.join(".git/shallow"), format!("{b}\n")).unwrap();
    let selected = real.repository.log_page(&b, 0, 1).unwrap().remove(0);
    assert_eq!(selected.parents, vec![c.clone()]);
    let workspace = support::workspace(
        &real,
        Comparison::CommitChanges {
            commit: b,
            parent: 0,
        },
    );
    let source = support::source(&workspace);
    assert_eq!(
        source.endpoint(OriginalComparisonSideV1::Old).commit(),
        Some(c.as_str())
    );
}
#[test]
fn original_diff_sides_member_raw_pair_duplicate_and_path_limit_preflight_before_display() {
    let (real, a, b, _) = support::fixture("sha1");
    let result = real
        .repository
        .compare(&Comparison::Snapshots { base: a, head: b })
        .unwrap();
    let mut invalid = result.clone();
    invalid.diff.files[0].old_path = Some(vec![b'a'; 4097]);
    assert!(preflight_members(&invalid).is_err());
    invalid = result.clone();
    invalid.diff.files.push(invalid.diff.files[0].clone());
    assert!(preflight_members(&invalid).is_err());
    invalid = result;
    invalid.patch.resize(ORIGINAL_DIFF_RAW_CAP_V1 + 1, b' ');
    assert!(preflight_members(&invalid).is_err());
}
#[test]
fn original_diff_sides_member_cap_is_partial_and_display_cap_does_not_invent_no_files() {
    let (real, a, b, _) = support::fixture("sha1");
    let mut result = real
        .repository
        .compare(&Comparison::Snapshots { base: a, head: b })
        .unwrap();
    let mut template = result.diff.files[0].clone();
    template.hunks.clear();
    template.metadata.clear();
    result.diff.files = (0..50_001)
        .map(|index| {
            let mut file = template.clone();
            file.old_path = None;
            file.new_path = Some(format!("member-{index:05}").into_bytes());
            file
        })
        .collect();
    let (admitted, bytes) = preflight_members(&result).unwrap();
    assert_eq!(admitted, 50_000);
    assert!(bytes <= ORIGINAL_DIFF_RAW_CAP_V1);
    assert!(preflight_diff_display(&result) < result.diff.files.len());
    assert!(!result.diff.files.is_empty());
}
#[test]
fn original_diff_sides_nul_raw_members_case_control_bytes_and_original_rename_paths() {
    let (real, _, _, c) = support::fixture("sha256");
    let tree = support::tree(
        &real,
        &[
            (b"Case", b"one\n", "100644"),
            (b"case", b"two\n", "100644"),
            (b"same-\xfe", b"FE\n", "100644"),
            (b"same-\xff", b"FF\n", "100644"),
            (b"line\nname", b"LF\n", "100644"),
        ],
    );
    let b = support::commit(&real, &tree, Some(&c), "Raw");
    let workspace = support::workspace(&real, Comparison::Snapshots { base: c, head: b });
    let source = support::source(&workspace);
    for path in [
        b"same-\xfe".as_slice(),
        b"same-\xff",
        b"Case",
        b"case",
        b"line\nname",
    ] {
        let row = source
            .members()
            .iter()
            .find(|row| {
                row.path(OriginalComparisonSideV1::New)
                    .is_some_and(|raw| raw.0 == path)
            })
            .unwrap();
        let detail = row.detail().join("");
        assert!(detail.contains(&display_path_bytes(path)));
    }
    assert!(source
        .members()
        .iter()
        .any(|row| row.label().contains("\\xfe")));
    assert!(source
        .members()
        .iter()
        .any(|row| row.label().contains("\\xff")));
}
#[test]
fn original_diff_sides_replaced_original_locator_stops_before_any_child() {
    let (real, a, b, _) = support::fixture("sha1");
    let workspace = support::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let intent = OriginalComparisonIntentV1::capture(&workspace).unwrap();
    let op = LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap();
    let old = real.root.join(".git");
    let held = real.root.join("original.git.saved-for-test");
    std::fs::rename(&old, &held).unwrap();
    std::fs::create_dir(&old).unwrap();
    real.clear_trace();
    assert!(OriginalComparisonSourceV1::read(intent, &op).is_err());
    assert!(real.children().is_empty());
    assert_eq!(op.accounting().unwrap().children_started(), 0);
    std::fs::remove_dir(&old).unwrap();
    std::fs::rename(held, old).unwrap();
}
