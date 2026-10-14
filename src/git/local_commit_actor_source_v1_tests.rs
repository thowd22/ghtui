use super::*;

#[test]
fn git_actor_association_local_real_worktree_locator_edit_is_not_original_directory_continuity() {
    let (dir, _, sha, _) = fixture(
        "sha1",
        "A <a> 1700000000 +0000",
        "C <c> 1700000100 +0000",
        "Original",
    );
    let linked = dir.path().join("linked");
    git(
        dir.path(),
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().unwrap(),
            &sha,
        ],
        None,
    );
    let repo = GitRepository::discover(&linked).unwrap().unwrap();
    let token =
        OriginalLocalCommitActors::read(&repo, &sha, &Default::default(), &Default::default())
            .unwrap();
    let actual_common = repo.common_dir.clone();
    let actual_git = repo.git_dir.clone();
    let locator = linked.join(".git");
    let original = fs::read(&locator).unwrap();
    fs::write(&locator, b"gitdir: /different/unavailable/source\n").unwrap();
    assert!(
        actual_common.is_dir() && actual_git.is_dir(),
        "Old directories still exist: their inode proof alone is insufficient"
    );
    assert!(token
        .recheck_original_source(&Default::default())
        .unwrap_err()
        .0
        .contains("locator"));
    fs::write(&locator, original).unwrap();
    token.recheck_original_source(&Default::default()).unwrap();
    assert_eq!(text(&linked, &["rev-parse", "HEAD"]), sha);
}
