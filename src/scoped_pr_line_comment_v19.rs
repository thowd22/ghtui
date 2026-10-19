//! "Comment line" from the PR Files diff: remember the selected diff line,
//! open the existing immutable file images, then select and verify that same
//! line there so the existing inline composer (P) is one step away. The line is
//! only a selection hint; the canonical range check still decides admission.
use super::{ui::PrImageViewerV1, PrImageSideV1};
use crate::{
    app::{Action, App},
    github_workspace::{GithubAction, PrTab},
    repository_view::{DiffAnchor, DiffSide},
    scoped_pr_observed_tabs_v1::resources::PrResourceUiActionV1,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineCommentTargetV19 {
    pub path: Vec<u8>,
    pub side: PrImageSideV1,
    pub line: u32,
}

pub const CHOOSE_LINE: &str =
    "Select a changed or context line in the diff (↑/↓ or click), then i Comment line";

/// Pure mapping from one rendered diff anchor to a commentable side/line.
/// Removed lines target LEFT; added and context lines target RIGHT.
pub fn target(anchor: &DiffAnchor) -> Option<LineCommentTargetV19> {
    let (side, line, path) = match anchor.side {
        DiffSide::Old => (
            PrImageSideV1::Old,
            anchor.old_line?,
            anchor.old_path.as_ref()?,
        ),
        DiffSide::New | DiffSide::Both => match anchor.new_line {
            Some(line) => (PrImageSideV1::New, line, anchor.new_path.as_ref()?),
            None => (
                PrImageSideV1::Old,
                anchor.old_line?,
                anchor.old_path.as_ref()?,
            ),
        },
    };
    (line > 0).then(|| LineCommentTargetV19 {
        path: path.0.clone(),
        side,
        line,
    })
}

fn selected_target(app: &App) -> Option<LineCommentTargetV19> {
    let page = app.github.as_ref()?.pr.as_ref()?;
    if page.tab != PrTab::Files {
        return None;
    }
    target(page.rows.get(app.inbox_list.selected)?.anchor.as_ref()?)
}

/// Both the visible control and the `i` key arrive here.
pub fn start(app: &mut App) {
    let Some(target) = selected_target(app) else {
        app.status = CHOOSE_LINE.into();
        return;
    };
    app.dispatch(Action::Github(GithubAction::Resource(
        PrResourceUiActionV1::OldImage,
    )));
    // Set after the synchronous capture: plain O clears any older target.
    app.pr_line_comment_target_v19 = Some(target);
}

/// Called right after a freshly prepared viewer is installed. A pending target
/// for a different file is discarded rather than guessed.
pub fn apply_pending(app: &mut App) {
    let Some(target) = app.pr_line_comment_target_v19.take() else {
        return;
    };
    if let Some(viewer) = app.pr_file_images.as_mut() {
        apply(viewer, &target);
    }
}

pub fn apply(viewer: &mut PrImageViewerV1, target: &LineCommentTargetV19) -> bool {
    let proof = viewer.original_proof_v1().clone();
    if proof.observed().image(target.side).path() != target.path.as_slice() {
        return false;
    }
    viewer.focus_line_v19(target.side, target.line)
}

#[cfg(test)]
#[path = "scoped_pr_line_comment_v19_tests.rs"]
mod tests;
