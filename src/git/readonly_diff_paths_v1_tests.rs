use super::*;
use crate::diff::Diff;
fn parsed(patch: &[u8]) -> Diff {
    Diff::parse(patch).unwrap()
}
#[test]
fn readonly_index_numstat_ordinary_raw_tabs_newlines_bytes_and_exact_order() {
    let diff=parsed(b"diff --git a/x b/x\n--- \"a/tab\\tline\\n\\376\"\n+++ \"b/tab\\tline\\n\\376\"\n@@ -1 +1 @@\n-old\n+new\n");
    assert_eq!(
        paths(&diff, b"1\t1\ttab\tline\n\xfe\0", 1024).unwrap(),
        vec![(
            Some(b"tab\tline\n\xfe".to_vec()),
            Some(b"tab\tline\n\xfe".to_vec())
        )]
    );
    assert!(paths(&diff, b"1\t1\ttab\tline\n\xff\0", 1024).is_err());
}
#[test]
fn readonly_index_numstat_empty_add_delete_mode_only_binary_are_metadata_not_counts() {
    let diff=parsed(b"diff --git a/add b/add\nnew file mode 100644\nindex 0000000..e69de29\ndiff --git a/delete b/delete\ndeleted file mode 100644\nindex e69de29..0000000\ndiff --git a/mode b/mode\nold mode 100644\nnew mode 100755\ndiff --git a/bin b/bin\nnew file mode 100644\nindex 0000000..1234567\nBinary files /dev/null and b/bin differ\n");
    assert_eq!(
        paths(
            &diff,
            b"0\t0\tadd\x000\t0\tdelete\x000\t0\tmode\0-\t-\tbin\0",
            1024
        )
        .unwrap(),
        vec![
            (None, Some(b"add".to_vec())),
            (Some(b"delete".to_vec()), None),
            (Some(b"mode".to_vec()), Some(b"mode".to_vec())),
            (None, Some(b"bin".to_vec()))
        ]
    );
}
#[test]
fn readonly_index_numstat_rename_sentinel_requires_same_decoded_canonical_pair() {
    let diff =
        parsed(b"diff --git a/old b/new\nsimilarity index 100%\nrename from old\nrename to new\n");
    assert_eq!(
        paths(&diff, b"0\t0\t\0old\0new\0", 1024).unwrap(),
        vec![(Some(b"old".to_vec()), Some(b"new".to_vec()))]
    );
    for raw in [
        b"0\t0\told\0".as_slice(),
        b"0\t0\t\0other\0new\0",
        b"0\t0\t\0new\0old\0",
        b"0\t0\t\0old\0old\0",
    ] {
        assert!(paths(&diff, raw, 1024).is_err());
    }
}
#[test]
fn readonly_index_numstat_refuses_truncated_missing_extra_duplicate_and_unpaired_binary() {
    let diff = parsed(b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\n");
    for raw in [
        b"1\t1\ta".as_slice(),
        b"1\ta\0",
        b"1\t1\t\0a\0",
        b"1\t1\t\0a\0b",
        b"1\t1\ta\0\0",
        b"1\t1\ta\x001\t1\ta\0",
        b"-\t1\ta\0",
        b"1\t-\ta\0",
        b"x\t1\ta\0",
        b"184467440737095516160\t1\ta\0",
        b"0\t0\ta\0",
    ] {
        assert!(paths(&diff, raw, 1024).is_err(), "{raw:?}");
    }
    assert!(paths(&diff, b"", 1024).is_err());
}
#[test]
fn readonly_index_numstat_duplicate_conflicting_malformed_mode_metadata_refuses() {
    for metadata in [
        "new file mode 100644\nnew file mode 100644",
        "new file mode 100644\ndeleted file mode 100644",
        "new file mode 10064",
        "new file mode 100648",
        "new file mode 100644 trailing",
    ] {
        let diff = parsed(format!("diff --git a/a b/a\n{metadata}\n").as_bytes());
        assert!(paths(&diff, b"0\t0\ta\0", 1024).is_err(), "{metadata}");
    }
}
#[test]
fn readonly_index_numstat_empty_comparison_and_preallocation_bounds() {
    assert!(paths(&Diff::default(), b"", 1024).unwrap().is_empty());
    assert!(paths(&Diff::default(), b"0\t0\ta\0", 1024).is_err());
    let mut diff = parsed(b"diff --git a/a b/a\nold mode 100644\nnew mode 100755\n");
    assert!(paths(&diff, b"0\t0\ta\0", 0).is_err());
    assert!(paths(&diff, b"0\t0\ta\0", 4).is_err());
    let oversized = format!("0\t0\t{}\0", "a".repeat(4097));
    assert!(paths(&diff, oversized.as_bytes(), 8192).is_err());
    diff.files = vec![diff.files[0].clone(); 50_001];
    assert!(paths(&diff, b"", 1024).is_err());
}

#[test]
fn readonly_index_numstat_unknown_or_malformed_only_metadata_never_invents_both_sides() {
    for metadata in [
        "index 1234567..7654321 100644",
        "unrecognized future metadata",
        "old mode 100644",
        "new mode 100755",
        "old mode 100644\nnew mode 10075",
        "old mode 100644\nold mode 100644\nnew mode 100755",
        "new file mode 100644\nold mode 100644\nnew mode 100755",
    ] {
        let diff = parsed(format!("diff --git a/a b/a\n{metadata}\n").as_bytes());
        assert!(paths(&diff, b"0\t0\ta\0", 1024).is_err(), "{metadata}");
    }
    let diff = parsed(b"diff --git a/a b/a\nold mode 100644\nnew mode 100755\n");
    assert_eq!(
        paths(&diff, b"0\t0\ta\0", 1024).unwrap(),
        vec![(Some(b"a".to_vec()), Some(b"a".to_vec()))]
    );
    let duplicate=parsed(b"diff --git a/a b/a\nold mode 100644\nnew mode 100755\ndiff --git a/a b/a\nold mode 100644\nnew mode 100755\n");
    assert!(paths(&duplicate,b"0\t0\ta\x000\t0\ta\0",1024).unwrap_err().to_string().contains("duplicate path pair"),"explicit duplicate protocol records refuse; not a claim installed Git emitted them for a typechange");
}
