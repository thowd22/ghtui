//! Genuine real Git source fixtures; keep returned Real alive through tests.
use crate::{
    git::{original_tree_inventory_v1::tests::fixture_tree, OriginalTreeIntentV1},
    local_commit_actor_origin_v1_test_support::{git, text, Real},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_tree_jump_v1::{OriginalTreeFrameV1, OriginalTreeJumpV1},
    workspace::{PreparedOriginalTreeNavigationV1, RepositoryMode, Workspace},
};
use std::{fs, sync::Arc};
pub(crate) fn ready(format: &str, paths: &[&[u8]]) -> (Real, Workspace, OriginalTreeJumpV1) {
    let real = Real::new(format);
    let workspace = fixture_tree(&real, paths);
    let jump = from_workspace(&workspace);
    real.clear_trace();
    (real, workspace, jump)
}
pub(crate) fn from_workspace(workspace: &Workspace) -> OriginalTreeJumpV1 {
    let mut jump = OriginalTreeJumpV1::new(OriginalTreeIntentV1::capture(workspace).unwrap());
    let request = jump.request().unwrap();
    let operation =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let frame = OriginalTreeFrameV1::prepare(request, &operation).unwrap();
    assert_eq!(operation.seal().unwrap().children_started(), 1);
    assert!(jump.accept(frame));
    jump
}
pub(crate) fn deep(
    format: &str,
) -> (
    Real,
    Workspace,
    OriginalTreeJumpV1,
    Arc<PreparedOriginalTreeNavigationV1>,
) {
    let real = Real::new(format);
    let path = (0..24)
        .map(|index| format!("deep-folder-{index:02}"))
        .collect::<Vec<_>>()
        .join("/")
        + "/actual.txt";
    let file = real.root.join(&path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, b"actual deep saved object\n").unwrap();
    git(&real.root, &["add", "--", "deep-folder-00"], None);
    git(&real.root, &["commit", "-qm", "Actual nested tree"], None);
    let sha = text(&real.root, &["rev-parse", "HEAD"]);
    let mut workspace = Workspace::load(&real.root, RepositoryMode::Tree, &sha, None)
        .unwrap()
        .unwrap();
    let jump = from_workspace(&workspace);
    let frame = jump.frame().unwrap();
    let row = frame
        .matches()
        .iter()
        .position(|row| frame.inventory().member(row.index()).unwrap().path().0 == path.as_bytes())
        .unwrap();
    let ticket = frame.ticket(row).unwrap();
    let operation =
        LocalGitNavigationReadV1::begin(ticket.inventory().intent().origin(), Default::default())
            .unwrap();
    let projection = PreparedOriginalTreeNavigationV1::jump(&ticket, &operation).unwrap();
    operation.seal().unwrap();
    projection.clone().install(&mut workspace).unwrap();
    real.clear_trace();
    (real, workspace, jump, projection)
}
