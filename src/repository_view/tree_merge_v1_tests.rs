use super::*;
fn entry(path: &[u8], id: &str) -> TreeEntry {
    TreeEntry {
        path: RawPath(path.into()),
        id: id.into(),
        kind: "blob".into(),
        mode: "100644".into(),
    }
}
#[test]
fn original_tree_jump_keyed_merge_exact_raw_paths_dedup_duplicates_and_preserve_order() {
    let id = "a".repeat(40);
    let mut tree = TreeBrowser::new("b".repeat(40), vec![entry(b"a", &id)]);
    tree.merge_bounded_keyed_v1(vec![
        entry(b"z", &id),
        entry(b"a", &id),
        entry(b"z", &id),
        entry(b"\xff", &id),
        entry(b"\xfe", &id),
    ])
    .unwrap();
    assert_eq!(
        tree.entries
            .iter()
            .map(|entry| entry.path.0.clone())
            .collect::<Vec<_>>(),
        vec![b"a".to_vec(), b"z".to_vec(), vec![0xfe], vec![0xff]]
    );
}
#[test]
fn original_tree_jump_keyed_merge_conflicting_object_or_oversize_refuses_atomically() {
    let id = "a".repeat(64);
    let mut tree = TreeBrowser::new(id.clone(), vec![entry(b"same", &id)]);
    let before = tree.entries.clone();
    assert!(tree
        .merge_bounded_keyed_v1(vec![entry(b"new", &id), entry(b"same", &"b".repeat(64))])
        .is_err());
    assert_eq!(tree.entries, before);
    assert!(tree
        .merge_bounded_keyed_v1(vec![entry(&vec![b'z'; 4097], &id)])
        .is_err());
    assert_eq!(tree.entries, before);
}
#[test]
fn original_tree_jump_keyed_merge_fifty_thousand_caps_and_expanded_directory_survives() {
    let id = "a".repeat(40);
    let dir = TreeEntry {
        path: RawPath(b"dir".to_vec()),
        id: id.clone(),
        kind: "tree".into(),
        mode: "040000".into(),
    };
    let mut tree = TreeBrowser::new(id.clone(), vec![dir]);
    tree.toggle(&RawPath(b"dir".to_vec()));
    let nodes = (0..50_000)
        .map(|index| entry(format!("dir/file{index:05}").as_bytes(), &id))
        .collect();
    assert!(tree.merge_bounded_keyed_v1(nodes).is_err());
    assert_eq!(tree.entries.len(), 1);
    assert!(tree.expanded_v1(&RawPath(b"dir".to_vec())));
}
