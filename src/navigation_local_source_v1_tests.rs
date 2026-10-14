use super::*;
use crate::{
    workspace::original_saved_navigation_v1::tests::{assert_no_discovery, NestedReal},
    workspace::{FileAction, RowAction},
};
use std::fs;

fn restore(fixture: &NestedReal, visit: &OriginalSavedVisitV1) -> Workspace {
    let operation = LocalGitNavigationReadV1::begin(visit.origin(), Default::default()).unwrap();
    fixture.real.clear_trace();
    let restored = visit.restore(&operation).unwrap();
    let children = fixture.real.children();
    assert!(
        !children.is_empty(),
        "Fresh exact route must actually read Git"
    );
    assert_no_discovery(&children);
    assert_eq!(operation.seal().unwrap().children_started(), children.len());
    assert!(Arc::ptr_eq(
        restored.actor_origin.as_ref().unwrap(),
        visit.origin()
    ));
    restored
}
#[test]
fn original_saved_navigation_visit_history_actor_restores_exact_member_role_sha_not_head() {
    for format in ["sha1", "sha256"] {
        let fixture = NestedReal::new(format);
        let before = fixture.real.snapshot();
        let mut workspace = fixture.workspace();
        fixture.file(&mut workspace);
        fixture.action(
            &mut workspace,
            RowAction::FileProvenance(FileAction::History),
        );
        fixture.action(
            &mut workspace,
            RowAction::FileProvenance(FileAction::HistoryEntry(fixture.responsible.clone())),
        );
        fixture.action(&mut workspace, RowAction::Actor(GitRole::Committer));
        let visit = OriginalSavedVisitV1::capture(&workspace).unwrap();
        let restored = restore(&fixture, &visit);
        assert_eq!(restored.actor_profile, Some(GitRole::Committer));
        assert_eq!(
            restored.selected_commit.as_ref().unwrap().id,
            fixture.responsible
        );
        assert_eq!(
            restored.file_provenance.as_ref().unwrap().revision,
            fixture.chosen
        );
        assert!(restored
            .rows
            .iter()
            .any(|row| row.label.contains("Current Local Recorder")));
        assert_eq!(fixture.real.snapshot(), before);
    }
}
#[test]
fn original_saved_navigation_visit_blame_page_original_path_line_and_back_are_exact() {
    let fixture = NestedReal::new("sha1");
    let before = fixture.real.snapshot();
    let mut workspace = fixture.workspace();
    fixture.file(&mut workspace);
    fixture.action(&mut workspace, RowAction::FileProvenance(FileAction::Blame));
    fixture.action(&mut workspace, RowAction::FileProvenance(FileAction::Next));
    fixture.action(
        &mut workspace,
        RowAction::FileProvenance(FileAction::BlameLine(102)),
    );
    let visit = OriginalSavedVisitV1::capture(&workspace).unwrap();
    let mut restored = restore(&fixture, &visit);
    let state = restored.file_provenance.as_ref().unwrap();
    assert_eq!(state.blame_start, 101);
    assert_eq!(state.selection.as_ref().unwrap().original_line, Some(102));
    assert_eq!(
        state.selection.as_ref().unwrap().path.as_ref().unwrap().0,
        b"history.txt"
    );
    fixture.action(&mut restored, RowAction::FileProvenance(FileAction::Back));
    assert_eq!(
        restored.file_provenance.as_ref().unwrap().view,
        ProvenanceView::Blame
    );
    assert_eq!(restored.revision, fixture.chosen);
    assert_eq!(fixture.real.snapshot(), before);
}
#[test]
fn original_saved_navigation_visit_forged_exact_tuple_or_missing_member_never_substitutes() {
    let fixture = NestedReal::new("sha1");
    let mut workspace = fixture.workspace();
    fixture.file(&mut workspace);
    fixture.action(
        &mut workspace,
        RowAction::FileProvenance(FileAction::BlameAt(2)),
    );
    let mut visit = OriginalSavedVisitV1::capture(&workspace).unwrap();
    // Module-private mutation only challenges the genuine producer's worker
    // recheck. No public constructor/parser can manufacture a route proof.
    Arc::get_mut(&mut visit)
        .unwrap()
        .route
        .provenance
        .as_mut()
        .unwrap()
        .selection
        .as_mut()
        .unwrap()
        .original_line = Some(103);
    let operation = LocalGitNavigationReadV1::begin(visit.origin(), Default::default()).unwrap();
    fixture.real.clear_trace();
    assert!(visit
        .restore(&operation)
        .unwrap_err()
        .contains("SHA/path/line/boundary changed"));
    assert_no_discovery(&fixture.real.children());
    assert_eq!(
        workspace.selected_commit.as_ref().unwrap().id,
        fixture.responsible
    );
}
#[test]
fn original_saved_navigation_visit_actor_route_distinct_role_and_source_registry_never_decodes_keys(
) {
    let fixture = NestedReal::new("sha256");
    let mut workspace = fixture.workspace();
    let tree = OriginalSavedVisitV1::capture(&workspace).unwrap();
    fixture.action(&mut workspace, RowAction::Actor(GitRole::Author));
    let author = OriginalSavedVisitV1::capture(&workspace).unwrap();
    fixture.action(&mut workspace, RowAction::Actor(GitRole::Committer));
    let committer = OriginalSavedVisitV1::capture(&workspace).unwrap();
    assert_ne!(tree.key(), author.key());
    assert_ne!(author.key(), committer.key());
    let mut registry = OriginalSavedVisitsV1::default();
    registry.remember(author.clone());
    registry.remember(committer.clone());
    assert!(Arc::ptr_eq(
        &registry.find(author.key(), &fixture.real.root).unwrap(),
        &author
    ));
    assert!(registry.find(b"HEAD", &fixture.real.root).is_none());
    assert!(registry
        .find(author.key(), fixture.real.directory.path())
        .is_none());
    registry.clear();
    assert!(registry.find(author.key(), &fixture.real.root).is_none());
}
#[test]
fn original_saved_navigation_visit_bounds_preflight_raw_paths_and_genuine_selected_members() {
    let fixture = NestedReal::new("sha1");
    let mut workspace = fixture.workspace();
    fixture.file(&mut workspace);
    workspace.preview.as_mut().unwrap().path = RawPath(vec![b'x'; 4097]);
    assert!(OriginalSavedVisitV1::capture(&workspace).is_err());
    workspace.preview.as_mut().unwrap().path = fixture.path.clone();
    fixture.action(
        &mut workspace,
        RowAction::FileProvenance(FileAction::BlameAt(2)),
    );
    workspace
        .file_provenance
        .as_mut()
        .unwrap()
        .selection
        .as_mut()
        .unwrap()
        .commit = fixture.chosen.clone();
    fixture.real.clear_trace();
    assert!(OriginalSavedVisitV1::capture(&workspace).is_err());
    assert!(fixture.real.children().is_empty());
}
#[test]
fn original_saved_navigation_visit_same_path_replacement_refuses_before_fresh_restore_child() {
    let fixture = NestedReal::new("sha1");
    let mut workspace = fixture.workspace();
    fixture.file(&mut workspace);
    let visit = OriginalSavedVisitV1::capture(&workspace).unwrap();
    let moved = fixture.real.directory.path().join("original-visit-kept");
    fs::rename(&fixture.real.root, &moved).unwrap();
    crate::local_commit_actor_origin_v1_test_support::git(
        fixture.real.directory.path(),
        &[
            "clone",
            "--quiet",
            "--no-hardlinks",
            moved.to_str().unwrap(),
            fixture.real.root.to_str().unwrap(),
        ],
        None,
    );
    let operation = LocalGitNavigationReadV1::begin(visit.origin(), Default::default()).unwrap();
    fixture.real.clear_trace();
    assert!(visit.restore(&operation).is_err());
    assert!(fixture.real.children().is_empty());
    assert_eq!(operation.accounting().unwrap().children_started(), 0);
    assert_eq!(
        fs::read(moved.join("nested/renamed.txt")).unwrap(),
        b"user dirty renamed bytes\n"
    );
}

#[test]
fn original_saved_navigation_visit_active_blame_actor_and_history_keys_ignore_inactive_cached_pages(
) {
    let fixture = NestedReal::new("sha256");
    let before = fixture.real.snapshot();
    let mut workspace = fixture.workspace();
    fixture.file(&mut workspace);
    fixture.action(
        &mut workspace,
        RowAction::FileProvenance(FileAction::History),
    );
    fixture.action(&mut workspace, RowAction::FileProvenance(FileAction::Blame));
    fixture.action(&mut workspace, RowAction::FileProvenance(FileAction::Next));
    fixture.action(
        &mut workspace,
        RowAction::FileProvenance(FileAction::BlameLine(102)),
    );
    fixture.action(&mut workspace, RowAction::Actor(GitRole::Author));
    assert!(
        workspace
            .file_provenance
            .as_ref()
            .unwrap()
            .history
            .is_some(),
        "the fixture genuinely retains inactive History rows"
    );
    let visit = OriginalSavedVisitV1::capture(&workspace).unwrap();
    let mut restored = restore(&fixture, &visit);
    let after = OriginalSavedVisitV1::capture(&restored).unwrap();
    assert_eq!(
        after.key(),
        visit.key(),
        "active exact source identity cannot depend on a hidden cached page"
    );
    assert_eq!(restored.actor_profile, Some(GitRole::Author));
    assert_eq!(
        restored.selected_commit.as_ref().unwrap().id,
        fixture.responsible
    );
    let state = restored.file_provenance.as_ref().unwrap();
    assert_eq!(state.blame_start, 101);
    assert_eq!(state.selection.as_ref().unwrap().original_line, Some(102));
    assert_eq!(
        state.selection.as_ref().unwrap().path.as_ref().unwrap().0,
        b"history.txt"
    );
    fixture.action(&mut restored, RowAction::FileProvenance(FileAction::Back));
    fixture.action(
        &mut restored,
        RowAction::FileProvenance(FileAction::History),
    );
    assert_eq!(
        restored.file_provenance.as_ref().unwrap().blame_start,
        101,
        "inactive blame position genuinely differs from its default"
    );
    let history = OriginalSavedVisitV1::capture(&restored).unwrap();
    let restored_history = restore(&fixture, &history);
    assert_eq!(
        OriginalSavedVisitV1::capture(&restored_history)
            .unwrap()
            .key(),
        history.key()
    );
    assert_eq!(
        restored_history
            .file_provenance
            .as_ref()
            .unwrap()
            .history
            .as_ref()
            .unwrap()
            .offset,
        0
    );
    assert_eq!(fixture.real.snapshot(), before);
}
