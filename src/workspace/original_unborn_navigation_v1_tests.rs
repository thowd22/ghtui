use super::*;
use crate::local_commit_actor_origin_v1_test_support::git;
use std::fs;

#[test]
fn original_saved_navigation_genuine_unborn_tree_graph_and_changes_have_no_invented_saved_sha() {
    let fixture = crate::test_support::Fixture::new();
    let root = fixture.repo();
    git(&root, &["init", "--initial-branch=main"], None);
    fs::write(root.join("scratch"), b"unborn untracked bytes\n").unwrap();
    let head = fs::read(root.join(".git/HEAD")).unwrap();
    let config = fs::read(root.join(".git/config")).unwrap();
    let index = fs::read(root.join(".git/index")).ok();
    let source = Workspace::load(&root, RepositoryMode::Overview, "HEAD", None)
        .unwrap()
        .unwrap();
    let origin = source.actor_origin.as_ref().unwrap();
    assert!(source.status.head.is_none());
    for mode in [
        RepositoryMode::Tree,
        RepositoryMode::Graph,
        RepositoryMode::CommitChanges,
    ] {
        let operation = LocalGitNavigationReadV1::begin(origin, Default::default()).unwrap();
        let result = source
            .load_original_with_operation_v1(mode, "HEAD", &operation)
            .unwrap();
        assert_eq!(result.mode, mode);
        assert_eq!(result.revision, "HEAD");
        assert!(result.status.head.is_none());
        assert!(result.selected_commit.is_none());
        assert!(Arc::ptr_eq(result.actor_origin.as_ref().unwrap(), origin));
        assert!(operation.seal().unwrap().children_started() > 0);
    }
    assert_eq!(fs::read(root.join(".git/HEAD")).unwrap(), head);
    assert_eq!(fs::read(root.join(".git/config")).unwrap(), config);
    assert_eq!(fs::read(root.join(".git/index")).ok(), index);
    assert_eq!(
        fs::read(root.join("scratch")).unwrap(),
        b"unborn untracked bytes\n"
    );
}
