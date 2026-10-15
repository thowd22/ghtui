//! Eight UNRUN pure canonical byte contracts; no raw test value is admission.
use super::*;
use crate::immutable_git_v1_test_support as history;
fn metadata(path: &str, status: &str, previous: Option<&str>) -> ObservedPrFileV1 {
    ObservedPrFileV1 {
        path: path.into(),
        previous_path: previous.map(str::to_owned),
        status: Some(status.into()),
        additions: None,
        deletions: None,
        blob_sha: None,
        patch_supplied: true,
    }
}
#[test]
fn native_external_diff_provider_retains_exact_selected_raw_section_and_digest_not_whole_pr() {
    use sha2::Digest;
    for sha256 in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (history, whole, _) = history::GitImagesV1::create_with_extra_file(
            directory.path(),
            sha256,
            history::OLD,
            history::NEW,
        );
        let selected = selected_patch(
            &whole,
            &metadata(history::PATH, "modified", None),
            &Default::default(),
        )
        .unwrap();
        assert_eq!(selected.raw.as_ref(), history.patch.as_slice());
        assert_ne!(selected.raw.as_ref(), whole.as_slice());
        assert_eq!(
            selected.digest,
            format!("{:x}", sha2::Sha256::digest(selected.raw.as_ref()))
        );
        assert!(!selected
            .raw
            .windows(b"z-other.rs".len())
            .any(|bytes| bytes == b"z-other.rs"));
        assert_eq!(
            selected.file.new_path.as_deref(),
            Some(history::PATH.as_bytes())
        );
    }
}
fn image(path: &str, bytes: &[u8], kind: PrImmutableImageKindV1) -> PrImmutableImageV1 {
    use sha1::Digest;
    let mut sha = sha1::Sha1::new();
    sha.update(format!("blob {}\0", bytes.len()));
    sha.update(bytes);
    PrImmutableImageV1 {
        path: path.as_bytes().to_vec(),
        blob: Some(format!("{:x}", sha.finalize())),
        mode: Some("100644".into()),
        bytes: Some(bytes.to_vec()),
        kind: Some(kind),
        absent: false,
    }
}
fn absent(path: &str) -> PrImmutableImageV1 {
    PrImmutableImageV1 {
        path: path.as_bytes().to_vec(),
        blob: None,
        mode: None,
        bytes: None,
        kind: None,
        absent: true,
    }
}
#[test]
fn immutable_object_full_sha1_sha256_tree_digest_from_actual_git_not_json() {
    for sha256 in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let h = history::GitImagesV1::create(dir.path(), sha256);
        for (sha, raw) in h.old_trees.iter().chain(&h.new_trees) {
            let mut entries = raw["tree"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| Entry {
                    path: row["path"].as_str().unwrap().as_bytes().to_vec(),
                    mode: row["mode"].as_str().unwrap().into(),
                    kind: row["type"].as_str().unwrap().into(),
                    sha: row["sha"].as_str().unwrap().into(),
                    size: row["size"].as_u64(),
                })
                .collect::<Vec<_>>();
            verify_tree(&mut entries, sha, 2 * 1024 * 1024).unwrap();
            entries[0].path.push(b'X');
            assert!(verify_tree(&mut entries, sha, 2 * 1024 * 1024).is_err());
        }
        assert!(crate::providers::gh::verify_blob_digest(history::OLD, &h.old_blob).is_ok());
        assert!(crate::providers::gh::verify_blob_digest(history::NEW, &h.new_blob).is_ok());
    }
}
#[test]
fn immutable_patch_empty_blob_distinct_from_absent_added_and_deleted_images() {
    for added in [false, true] {
        let status = if added { "added" } else { "removed" };
        let raw = if added {
            b"diff --git a/empty b/empty\nnew file mode 100644\n".as_slice()
        } else {
            b"diff --git a/empty b/empty\ndeleted file mode 100644\n".as_slice()
        };
        let m = metadata("empty", status, None);
        let selected = selected_patch(raw, &m, &Default::default()).unwrap();
        let empty = image("empty", b"", PrImmutableImageKindV1::Text);
        let missing = absent("empty");
        let (old, new) = if added {
            (&missing, &empty)
        } else {
            (&empty, &missing)
        };
        assert!(!associate_patch(&selected, &m, old, new, &Default::default()).unwrap());
        assert_ne!(old.absent(), new.absent());
        assert_eq!(empty.bytes(), Some(b"".as_slice()));
        assert!(missing.bytes().is_none());
    }
}
#[test]
fn immutable_patch_rename_and_copy_exact_path_old_new_bytes_and_ranges() {
    for copied in [false, true] {
        let family = if copied { "copy" } else { "rename" };
        let status = if copied { "copied" } else { "renamed" };
        let raw=format!("diff --git a/old name b/new name\nsimilarity index 100%\n{family} from old name\n{family} to new name\n");
        let m = metadata("new name", status, Some("old name"));
        let selected = selected_patch(raw.as_bytes(), &m, &Default::default()).unwrap();
        assert_eq!(selected.old_path, Some(b"old name".to_vec()));
        assert_eq!(selected.new_path, Some(b"new name".to_vec()));
        assert!(!associate_patch(
            &selected,
            &m,
            &image("old name", b"same\n", PrImmutableImageKindV1::Text),
            &image("new name", b"same\n", PrImmutableImageKindV1::Text),
            &Default::default()
        )
        .unwrap());
        assert!(associate_patch(
            &selected,
            &m,
            &image("old name", b"wrong\n", PrImmutableImageKindV1::Text),
            &image("new name", b"same\n", PrImmutableImageKindV1::Text),
            &Default::default()
        )
        .is_err());
    }
}
#[test]
fn immutable_patch_exact_crlf_no_newline_and_counter_index_contradictions() {
    let old = image("file", b"old\r\n", PrImmutableImageKindV1::Text);
    let new = image("file", b"new", PrImmutableImageKindV1::Text);
    let mut m = metadata("file", "modified", None);
    m.additions = Some(1);
    m.deletions = Some(1);
    let raw=format!("diff --git a/file b/file\nindex {}..{} 100644\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\r\n+new\n\\ No newline at end of file\n",old.blob_sha().unwrap(),new.blob_sha().unwrap());
    let selected = selected_patch(raw.as_bytes(), &m, &Default::default()).unwrap();
    assert!(associate_patch(&selected, &m, &old, &new, &Default::default()).unwrap());
    m.additions = Some(2);
    assert!(associate_patch(&selected, &m, &old, &new, &Default::default()).is_err());
    m.additions = Some(1);
    let wrong = raw.replace(old.blob_sha().unwrap(), &"f".repeat(40));
    let selected = selected_patch(wrong.as_bytes(), &m, &Default::default()).unwrap();
    assert!(associate_patch(&selected, &m, &old, &new, &Default::default()).is_err());
}
#[test]
fn immutable_patch_binary_symlink_submodule_and_lfs_never_text_range() {
    for kind in [
        PrImmutableImageKindV1::Binary,
        PrImmutableImageKindV1::Symlink,
        PrImmutableImageKindV1::Submodule,
        PrImmutableImageKindV1::LfsPointer,
    ] {
        let m = metadata("file", "modified", None);
        let raw = b"diff --git a/file b/file\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        let mut selected = selected_patch(raw, &m, &Default::default()).unwrap();
        if kind == PrImmutableImageKindV1::Binary {
            Arc::make_mut(&mut selected.file).binary = true;
        }
        assert!(!associate_patch(
            &selected,
            &m,
            &image("file", b"old\n", kind),
            &image("file", b"new\n", kind),
            &Default::default()
        )
        .unwrap());
    }
}
#[test]
fn immutable_patch_utf8_api_paths_exact_quoted_git_bytes_not_lossy_or_casefolded() {
    let m = metadata("界.rs", "modified", None);
    let raw=b"diff --git \"a/\\347\\225\\214.rs\" \"b/\\347\\225\\214.rs\"\n--- \"a/\\347\\225\\214.rs\"\n+++ \"b/\\347\\225\\214.rs\"\n@@ -1 +1 @@\n-old\n+new\n";
    // Literal octal escapes must reach the byte decoder, not Rust escapes.
    let raw = std::str::from_utf8(raw).unwrap().to_owned();
    assert!(selected_patch(raw.as_bytes(), &m, &Default::default()).is_ok());
    let mut invalid = m.clone();
    invalid.path = "../界.rs".into();
    assert!(selected_patch(raw.as_bytes(), &invalid, &Default::default()).is_err());
    let changed = raw.replace(".rs", ".RS");
    assert!(selected_patch(changed.as_bytes(), &m, &Default::default()).is_err());
}
#[test]
fn immutable_patch_hunk_overflow_count_mismatch_and_duplicate_sections_refuse() {
    let m = metadata("file", "modified", None);
    let raw = b"diff --git a/file b/file\n--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-old\n+new\n";
    assert!(selected_patch(raw, &m, &Default::default()).is_err());
    let valid = String::from_utf8(raw.to_vec())
        .unwrap()
        .replace("-1,2 +1,2", "-1,1 +1,1");
    let mut selected = selected_patch(valid.as_bytes(), &m, &Default::default()).unwrap();
    Arc::make_mut(&mut selected.file).hunks[0].old_count = 2;
    assert!(associate_patch(
        &selected,
        &m,
        &image("file", b"old\n", PrImmutableImageKindV1::Text),
        &image("file", b"new\n", PrImmutableImageKindV1::Text),
        &Default::default()
    )
    .is_err());
    let overflow = String::from_utf8(raw.to_vec())
        .unwrap()
        .replace("-1,2", "-4294967295,2");
    assert!(selected_patch(overflow.as_bytes(), &m, &Default::default()).is_err());
    assert!(selected_patch(
        &[raw.as_slice(), raw.as_slice()].concat(),
        &m,
        &Default::default()
    )
    .is_err());
}
#[test]
fn immutable_patch_wrong_status_modes_or_partial_rest_membership_cannot_mint_range() {
    let m = metadata("file", "modified", None);
    let raw = b"diff --git a/file b/file\nnew file mode 100644\n";
    assert!(selected_patch(raw, &m, &Default::default()).is_err());
    assert!(selected_patch(b"diff --git a/other b/other\n", &m, &Default::default()).is_err());
    let limits = PrImmutableFileLimitsV1 {
        max_path_depth: 1,
        ..Default::default()
    };
    assert!(selected_patch(
        b"diff --git a/src/file b/src/file\n",
        &metadata("src/file", "modified", None),
        &limits
    )
    .is_err());
}
