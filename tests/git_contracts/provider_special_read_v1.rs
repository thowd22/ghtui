use super::{git, Fixture, GitRepository, RawPath};
use std::{ffi::OsStr, fs, path::Path};

fn optional_bytes(path: &Path) -> Option<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("Cannot inspect {}: {error}", path.display()),
    }
}

#[test]
fn git_provider_sparse_reads_saved_outside_cone_without_populating_or_changing_checkout() {
    let fixture = Fixture::linear();
    fixture.write("visible/inside.txt", b"saved visible bytes\n");
    fixture.write(
        "outside/sparse-only.txt",
        b"saved outside sparse checkout\n",
    );
    fixture.commit("Saved sparse tree");
    git(
        &fixture.root,
        ["sparse-checkout", "set", "--cone", "visible"],
    );
    assert!(!fixture.root.join("outside/sparse-only.txt").exists());
    fixture.write("visible/inside.txt", b"dirty visible bytes stay local\n");
    fixture.write("untracked.txt", b"untracked bytes stay local\n");
    let head = fixture.head();
    let index = fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let config = fs::read(fixture.repository.git_dir.join("config")).unwrap();
    let worktree_config = optional_bytes(&fixture.repository.git_dir.join("config.worktree"));
    let patterns = fs::read(fixture.repository.git_dir.join("info/sparse-checkout")).unwrap();
    let refs = git(&fixture.root, ["show-ref"]).stdout;
    let before = fixture.repository.status().unwrap();
    assert!(before.sparse);
    assert_eq!(before.head.as_deref(), Some(head.as_str()));
    assert!(before
        .entries
        .iter()
        .any(|entry| entry.path.0 == b"visible/inside.txt"));
    assert!(!before
        .entries
        .iter()
        .any(|entry| entry.path.0 == b"outside/sparse-only.txt"));
    let outside = RawPath::new(b"outside/sparse-only.txt".to_vec());
    let inside = RawPath::new(b"visible/inside.txt".to_vec());
    let omitted = fixture.repository.tree_entry(&head, &outside).unwrap();
    let present = fixture.repository.tree_entry(&head, &inside).unwrap();
    assert_eq!(omitted.kind, "blob");
    assert!(fixture
        .repository
        .tree_children(&head, Some(&RawPath::new(b"outside".to_vec())))
        .unwrap()
        .iter()
        .any(|entry| entry.path == outside && entry.id == omitted.id));
    assert_eq!(
        fixture.repository.blob(&head, &outside).unwrap(),
        b"saved outside sparse checkout\n"
    );
    let ids = [omitted.id, present.id];
    assert_eq!(
        fixture.repository.objects(&ids).unwrap(),
        vec![
            b"saved outside sparse checkout\n".to_vec(),
            b"saved visible bytes\n".to_vec()
        ]
    );
    assert_eq!(
        fixture.repository.object_sizes(&ids).unwrap(),
        vec![
            b"saved outside sparse checkout\n".len(),
            b"saved visible bytes\n".len()
        ]
    );
    assert_eq!(fixture.repository.status().unwrap(), before);
    assert_eq!(fixture.head(), head);
    assert_eq!(git(&fixture.root, ["show-ref"]).stdout, refs);
    assert_eq!(
        fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
    assert_eq!(
        fs::read(fixture.repository.git_dir.join("config")).unwrap(),
        config
    );
    assert_eq!(
        optional_bytes(&fixture.repository.git_dir.join("config.worktree")),
        worktree_config
    );
    assert_eq!(
        fs::read(fixture.repository.git_dir.join("info/sparse-checkout")).unwrap(),
        patterns
    );
    assert_eq!(
        fs::read(fixture.root.join("visible/inside.txt")).unwrap(),
        b"dirty visible bytes stay local\n"
    );
    assert_eq!(
        fs::read(fixture.root.join("untracked.txt")).unwrap(),
        b"untracked bytes stay local\n"
    );
    assert!(!fixture.root.join("outside").exists());
    assert!(!fixture.repository.git_dir.join("index.lock").exists());
}

#[test]
fn git_provider_bare_reads_immutable_objects_without_inventing_working_files_or_index() {
    let fixture = Fixture::linear();
    let head = fixture.head();
    let source_index = fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let bare = fixture.directory.path().join("saved.git");
    git(
        fixture.directory.path(),
        [
            OsStr::new("clone"),
            OsStr::new("--bare"),
            OsStr::new("--no-hardlinks"),
            fixture.root.as_os_str(),
            bare.as_os_str(),
        ],
    );
    let repository = GitRepository::discover(&bare).unwrap().unwrap();
    assert!(repository.bare);
    assert_eq!(repository.root, fs::canonicalize(&bare).unwrap());
    let bare_head = fs::read(bare.join("HEAD")).unwrap();
    let config = fs::read(bare.join("config")).unwrap();
    let refs = git(&bare, ["show-ref"]).stdout;
    assert!(!bare.join("index").exists());
    assert_eq!(repository.resolve_original("HEAD").unwrap(), head);
    let path = RawPath::new(b"readme.txt".to_vec());
    let entry = repository.tree_entry(&head, &path).unwrap();
    assert!(repository
        .tree_children(&head, None)
        .unwrap()
        .iter()
        .any(|row| row.path == path && row.id == entry.id));
    assert_eq!(repository.blob(&head, &path).unwrap(), b"first\nchanged\n");
    assert_eq!(
        repository.objects(std::slice::from_ref(&entry.id)).unwrap(),
        vec![b"first\nchanged\n".to_vec()]
    );
    assert_eq!(
        repository
            .object_sizes(std::slice::from_ref(&entry.id))
            .unwrap(),
        vec![14]
    );
    assert!(repository
        .references()
        .unwrap()
        .iter()
        .any(|reference| reference.id == head && reference.raw_name == b"refs/heads/main"));
    assert!(repository
        .status()
        .unwrap_err()
        .0
        .contains("bare repository has no working files or index"));
    assert_eq!(fs::read(bare.join("HEAD")).unwrap(), bare_head);
    assert_eq!(fs::read(bare.join("config")).unwrap(), config);
    assert_eq!(git(&bare, ["show-ref"]).stdout, refs);
    assert!(!bare.join("index").exists());
    assert!(!bare.join("index.lock").exists());
    assert!(!bare.join("readme.txt").exists());
    assert_eq!(fixture.head(), head);
    assert_eq!(
        fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        source_index
    );
    assert_eq!(
        fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"first\nchanged\n"
    );
}
