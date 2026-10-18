use super::*;
use crate::git::RawPath;

fn anchor(side: DiffSide, old: Option<u32>, new: Option<u32>) -> DiffAnchor {
    DiffAnchor {
        base: None,
        head: None,
        old_path: Some(RawPath(b"src/old.rs".to_vec())),
        new_path: Some(RawPath(b"src/new.rs".to_vec())),
        old_line: old,
        new_line: new,
        side,
        file_index: 0,
        hunk_index: 0,
        line_index: 0,
    }
}

#[test]
fn v1_pr_threads_line_comment_target_maps_removed_left_and_added_or_context_right() {
    let removed = target(&anchor(DiffSide::Old, Some(7), None)).unwrap();
    assert_eq!(
        (removed.side, removed.line, removed.path.as_slice()),
        (PrImageSideV1::Old, 7, b"src/old.rs".as_slice())
    );
    let added = target(&anchor(DiffSide::New, None, Some(9))).unwrap();
    assert_eq!(
        (added.side, added.line, added.path.as_slice()),
        (PrImageSideV1::New, 9, b"src/new.rs".as_slice())
    );
    let context = target(&anchor(DiffSide::Both, Some(3), Some(4))).unwrap();
    assert_eq!((context.side, context.line), (PrImageSideV1::New, 4));
    assert!(target(&anchor(DiffSide::Old, None, Some(1))).is_none());
    assert!(target(&anchor(DiffSide::New, None, Some(0))).is_none());
}

#[test]
fn v1_pr_threads_line_comment_without_a_files_line_explains_and_starts_nothing() {
    let mut app = App::default();
    start(&mut app);
    assert_eq!(app.status, CHOOSE_LINE);
    assert!(app.pr_line_comment_target_v19.is_none());
    assert!(app.github_action.is_none());
    assert!(app.overlay.is_none());
}
