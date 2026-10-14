use super::*;
use crate::{
    local_commit_actor_origin_v1_test_support::{git, text, Real},
    workspace::{RepositoryMode, Workspace},
};

pub(crate) fn fixture_tree(real: &Real, paths: &[&[u8]]) -> Workspace {
    let id = text(
        &real.root,
        &["rev-parse", &format!("{}:tracked", real.original)],
    );
    let mut records = vec![];
    for path in paths {
        records.extend_from_slice(format!("100644 blob {id}\t").as_bytes());
        records.extend_from_slice(path);
        records.push(0);
    }
    let tree = String::from_utf8(git(&real.root, &["mktree", "-z"], Some(&records)))
        .unwrap()
        .trim()
        .to_owned();
    let sha = String::from_utf8(git(
        &real.root,
        &[
            "commit-tree",
            &tree,
            "-p",
            &real.head,
            "-m",
            "Original bounded tree source",
        ],
        None,
    ))
    .unwrap()
    .trim()
    .to_owned();
    Workspace::load(&real.root, RepositoryMode::Tree, &sha, None)
        .unwrap()
        .unwrap()
}
#[test]
fn original_tree_jump_preflight_caps_preserve_raw_records_and_honest_partial_members() {
    let sha = "a".repeat(40);
    let mut bytes = vec![];
    for index in 0..50_001 {
        bytes.extend_from_slice(format!("100644 blob {sha}\tfile-{index:05}\0").as_bytes());
    }
    let parsed = preflight(&bytes).unwrap();
    assert_eq!(parsed.observed, 50_001);
    assert_eq!(parsed.rows.len(), 50_000);
    assert_eq!(parsed.excluded, 1);
    let raw = b"space\nraw\t\xff";
    let mut bytes = format!("100644 blob {sha}\t").into_bytes();
    bytes.extend_from_slice(raw);
    bytes.push(0);
    assert_eq!(preflight(&bytes).unwrap().rows[0].path, raw);
    for raw in [
        b"a\\\n\r\t\xff".as_slice(),
        "raw\u{202e}日本".as_bytes(),
        b"incomplete-\xe2\x82",
    ] {
        assert_eq!(display_bound(raw), display_path_bytes(raw).len());
        let mut record = format!("100644 blob {sha}\t").into_bytes();
        record.extend_from_slice(raw);
        record.push(0);
        let charged = preflight(&record).unwrap().prepared_bytes;
        assert_eq!(
            charged,
            raw.len()
                + 2 * display_path_bytes(raw).len()
                + "100644".len()
                + "blob".len()
                + sha.len()
        );
    }
}
#[test]
fn original_tree_jump_preflight_mode_type_duplicate_and_incomplete_records_refuse() {
    let sha = "a".repeat(64);
    for body in [
        format!("100644 commit {sha}\tx\0"),
        format!("040000 blob {sha}\tx\0"),
        format!("100644 blob {sha}\tx"),
        format!("100644 blob {sha}\tx\0100644 blob {sha}\tx\0"),
    ] {
        assert!(preflight(body.as_bytes()).is_err());
    }
    let bytes = format!("100644 blob {sha}\ta\0100644 blob {sha}\tb\0");
    assert_eq!(
        preflight(bytes.as_bytes()).unwrap().rows.len(),
        2,
        "Same actual blob can have distinct raw paths"
    );
}
#[test]
fn original_tree_jump_preflight_oversized_path_is_excluded_not_complete_empty() {
    let sha = "a".repeat(40);
    let bytes = format!("100644 blob {sha}\t{}\0", "x".repeat(4097));
    let parsed = preflight(bytes.as_bytes()).unwrap();
    assert_eq!(parsed.observed, 1);
    assert_eq!(parsed.excluded, 1);
    assert!(parsed.rows.is_empty());
    assert!(preflight(&vec![1; RAW_CAP + 1]).is_err());
}
#[test]
fn original_tree_jump_actual_sha_algorithms_case_control_and_nonutf8_paths_no_discovery() {
    for format in ["sha1", "sha256"] {
        let real = Real::new(format);
        let before = real.snapshot();
        let workspace = fixture_tree(
            &real,
            &[
                b"Case.rs",
                b"case.rs",
                b"same-\xff",
                b"same-\xfe",
                b"line\nname",
            ],
        );
        let intent = OriginalTreeIntentV1::capture(&workspace).unwrap();
        let operation =
            LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap();
        real.clear_trace();
        let inventory = OriginalTreeInventoryV1::read(intent.clone(), &operation).unwrap();
        assert!(inventory.complete());
        assert_eq!(inventory.members().len(), 5);
        assert!(Arc::ptr_eq(inventory.intent(), &intent));
        assert_eq!(operation.seal().unwrap().children_started(), 1);
        assert_eq!(real.children().len(), 1);
        for path in [
            b"Case.rs".as_slice(),
            b"case.rs",
            b"same-\xff",
            b"same-\xfe",
            b"line\nname",
        ] {
            assert!(inventory
                .members()
                .iter()
                .any(|member| member.path().0 == path));
        }
        assert_eq!(real.snapshot(), before);
    }
}
#[test]
fn original_tree_jump_actual_source_replacement_and_cancel_refuse_before_child() {
    let real = Real::new("sha1");
    let workspace = fixture_tree(&real, &[b"saved"]);
    let intent = OriginalTreeIntentV1::capture(&workspace).unwrap();
    let token = CancellationToken::default();
    let op = LocalGitNavigationReadV1::begin(intent.origin(), token.clone()).unwrap();
    token.cancel();
    real.clear_trace();
    assert!(OriginalTreeInventoryV1::read(intent.clone(), &op).is_err());
    assert!(real.children().is_empty());
    let held = real.directory.path().join("moved-original");
    std::fs::rename(&real.root, &held).unwrap();
    git(
        real.directory.path(),
        &[
            "clone",
            "--quiet",
            "--no-hardlinks",
            held.to_str().unwrap(),
            real.root.to_str().unwrap(),
        ],
        None,
    );
    let op = LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap();
    assert!(OriginalTreeInventoryV1::read(intent, &op)
        .unwrap_err()
        .contains("source/locator changed"));
    assert_eq!(op.accounting().unwrap().children_started(), 0);
}
#[test]
fn original_tree_jump_no_origin_wrong_mode_or_selected_revision_cannot_capture_intent() {
    let real = Real::new("sha1");
    let mut workspace = fixture_tree(&real, &[b"saved"]);
    workspace.mode = RepositoryMode::Working;
    assert!(OriginalTreeIntentV1::capture(&workspace).is_err());
    workspace.mode = RepositoryMode::Tree;
    workspace.revision = real.head.clone();
    assert!(OriginalTreeIntentV1::capture(&workspace).is_err());
    workspace.actor_origin = None;
    assert!(OriginalTreeIntentV1::capture(&workspace).is_err());
}

#[test]
fn original_tree_jump_actual_row_cap_is_partial_not_complete_empty_or_false_absence() {
    let real = Real::new("sha1");
    let paths = (0..50_001)
        .map(|index| format!("file-{index:05}").into_bytes())
        .collect::<Vec<_>>();
    let refs = paths.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let workspace = fixture_tree(&real, &refs);
    let before = real.snapshot();
    let intent = OriginalTreeIntentV1::capture(&workspace).unwrap();
    let op = LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap();
    real.clear_trace();
    let inventory = OriginalTreeInventoryV1::read(intent, &op).unwrap();
    assert_eq!(inventory.observed_rows(), 50_001);
    assert_eq!(inventory.members().len(), 50_000);
    assert_eq!(inventory.excluded_rows(), 1);
    assert!(inventory.prepared_bytes() <= RAW_CAP);
    assert!(!inventory.complete());
    assert!(inventory.member(49_999).is_some() && inventory.member(50_000).is_none());
    assert_eq!(op.seal().unwrap().children_started(), 1);
    assert_eq!(real.children().len(), 1);
    assert_eq!(real.snapshot(), before);
}

#[test]
fn original_tree_jump_actual_capture_limit_reserves_eight_mib_without_partial_inventory() {
    let real = Real::new("sha256");
    let paths = (0..40_000)
        .map(|index| format!("file-{index:05}-{}", "x".repeat(180)).into_bytes())
        .collect::<Vec<_>>();
    let refs = paths.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let workspace = fixture_tree(&real, &refs);
    let before = real.snapshot();
    let intent = OriginalTreeIntentV1::capture(&workspace).unwrap();
    let op = LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap();
    real.clear_trace();
    assert!(OriginalTreeInventoryV1::read(intent, &op).is_err());
    let accounting = op.seal().unwrap();
    assert_eq!(accounting.children_started(), 1);
    assert_eq!(accounting.failed_reserved_bytes(), RAW_CAP);
    assert_eq!(real.children().len(), 1);
    assert_eq!(real.snapshot(), before);
}
