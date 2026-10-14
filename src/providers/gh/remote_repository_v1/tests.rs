//! UNRUN private parser contracts use independent actual Git objects.
use super::*;
use crate::{remote_repository_v1_test_support as fx, test_support::Fixture};
use serde_json::json;

#[test]
fn remote_tree_v1_complete_directory_matches_real_git_order_modes_and_digest() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    assert_eq!(
        fx::git(&fixture.repo(), &["cat-file", "-t", &objects.tag]),
        b"tag\n"
    );
    let (entries, complete) = parse::directory(
        objects.root_json.clone(),
        &objects.tree,
        &Default::default(),
    )
    .unwrap();
    assert!(complete);
    assert!(entries
        .iter()
        .any(|e| e.name() == "nested" && e.kind() == RemoteEntryKindV1::Directory));
    assert!(entries
        .iter()
        .any(|e| e.name() == "link" && e.kind() == RemoteEntryKindV1::Symlink));
    let mut reordered = objects.root_json.clone();
    reordered["tree"].as_array_mut().unwrap().reverse();
    assert!(
        parse::directory(reordered, &objects.tree, &Default::default())
            .unwrap()
            .1
    );
    fx::preserved(&fixture, &objects);
}
#[test]
fn remote_tree_v1_forged_complete_membership_and_type_never_verify() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    for mode in 0..4 {
        let mut raw = objects.root_json.clone();
        match mode {
            0 => {
                raw["tree"].as_array_mut().unwrap().remove(0);
            }
            1 => raw["tree"][0]["sha"] = json!("a".repeat(40)),
            2 => raw["tree"][0]["mode"] = json!("777777"),
            _ => raw["truncated"] = Value::Null,
        }
        assert!(parse::directory(raw, &objects.tree, &Default::default()).is_err());
    }
    fx::preserved(&fixture, &objects);
}
#[test]
fn remote_tree_v1_partial_reported_rows_never_become_complete_or_zero_absence() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut raw = objects.root_json.clone();
    raw["truncated"] = json!(true);
    raw["tree"].as_array_mut().unwrap().truncate(1);
    let (rows, complete) = parse::directory(raw, &objects.tree, &Default::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!complete);
    let limits = RemoteRepositoryLimitsV1 {
        max_directory_rows: 1,
        ..Default::default()
    };
    assert!(parse::directory(objects.root_json.clone(), &objects.tree, &limits).is_err());
    fx::preserved(&fixture, &objects);
}
#[test]
fn remote_tree_v1_literal_unicode_paths_percent_refs_and_sha_algorithms_stay_distinct() {
    assert_eq!(parse::encoded("release/雪%2F"), "release%2F%E9%9B%AA%252F");
    assert_ne!(parse::encoded("x/y"), parse::encoded("x%2Fy"));
    assert_eq!(
        parse::joined("nested", "日本語.txt").unwrap(),
        "nested/日本語.txt"
    );
    for bad in ["../x", "/x", ".", "..", "x\0y"] {
        assert!(parse::joined("nested", bad).is_err());
    }
    for bad in ["refs//bad", "bad..ref", "foo.lock", "@{-1}", "bad\nref"] {
        assert!(parse::reference_name(bad).is_err());
    }
    for bad in [
        "",
        "HEAD",
        "0000000000000000000000000000000000000000",
        "A000000000000000000000000000000000000000",
    ] {
        assert!(parse::oid(bad).is_err());
    }
    let blob = b"real64SHA bytes\n";
    use sha2::Digest;
    let mut bytes = format!("blob {}\0", blob.len()).into_bytes();
    bytes.extend(blob);
    let digest = format!("{:x}", sha2::Sha256::digest(bytes));
    assert!(verify_blob_digest(blob, &digest).is_ok());
    assert!(verify_blob_digest(b"wrong", &digest).is_err());
}
#[test]
fn remote_tree_v1_raw_metadata_bounds_precede_display_or_member_copies() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let limits = RemoteRepositoryLimitsV1 {
        max_metadata_bytes: 8,
        ..Default::default()
    };
    assert!(parse::directory(objects.root_json.clone(), &objects.tree, &limits).is_err());
    let mut raw = objects.child_json.clone();
    raw["tree"][0]["path"] = json!("界".repeat(1400));
    assert!(parse::directory(raw, &objects.child, &Default::default()).is_err());
    assert!(RemoteRepositoryLimitsV1 {
        max_refs: 1,
        ..Default::default()
    }
    .validate()
    .is_err());
    fx::preserved(&fixture, &objects);
}
