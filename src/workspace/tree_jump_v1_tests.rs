use super::*;
use crate::{
    git::{OriginalTreeIntentV1, OriginalTreeInventoryV1},
    repository_tree_jump_v1::{OriginalTreeFrameV1, OriginalTreeJumpV1},
    workspace::original_saved_navigation_v1::tests::NestedReal,
};

fn inventory(fixture: &NestedReal) -> Arc<OriginalTreeInventoryV1> {
    let workspace = fixture.workspace();
    inventory_from(&workspace)
}
fn inventory_from(workspace: &Workspace) -> Arc<OriginalTreeInventoryV1> {
    let intent = OriginalTreeIntentV1::capture(workspace).unwrap();
    let op = LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap();
    let inventory = OriginalTreeInventoryV1::read(intent, &op).unwrap();
    op.seal().unwrap();
    inventory
}
fn jump_ticket(fixture: &NestedReal, needle: &str) -> OriginalTreeTicketV1 {
    let intent = OriginalTreeIntentV1::capture(&fixture.workspace()).unwrap();
    let mut state = OriginalTreeJumpV1::new(intent);
    state
        .apply(crate::repository_tree_jump_v1::OriginalTreeJumpActionV1::Insert(needle.into()))
        .unwrap();
    let request = state.pending().unwrap().clone();
    let op =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let frame = OriginalTreeFrameV1::prepare(request, &op).unwrap();
    op.seal().unwrap();
    assert!(state.accept(frame.clone()));
    frame.ticket(0).unwrap()
}
#[test]
fn original_tree_jump_projection_exact_nested_file_source_sha_blob_and_typed_breadcrumbs() {
    for format in ["sha1", "sha256"] {
        let fixture = NestedReal::new(format);
        let before = fixture.real.snapshot();
        let ticket = jump_ticket(&fixture, "nested/renamed.txt");
        let op = LocalGitNavigationReadV1::begin(
            ticket.inventory().intent().origin(),
            Default::default(),
        )
        .unwrap();
        fixture.real.clear_trace();
        let prepared = PreparedOriginalTreeNavigationV1::jump(&ticket, &op).unwrap();
        assert_eq!(prepared.preview().unwrap().path, fixture.path);
        assert_eq!(prepared.preview().unwrap().revision, fixture.chosen);
        assert!(prepared
            .preview()
            .unwrap()
            .content
            .starts_with(b"actual saved line 001\n"));
        assert!(prepared
            .breadcrumbs()
            .iter()
            .any(|crumb| crumb.label() == "[Folder: nested]"));
        assert_eq!(
            op.seal().unwrap().children_started(),
            fixture.real.children().len()
        );
        assert!(!fixture.real.children().is_empty());
        assert_eq!(fixture.real.snapshot(), before);
    }
}

fn opened_nested_tree_file(fixture: &NestedReal) -> Workspace {
    let mut workspace = fixture.workspace();
    let inventory = inventory_from(&workspace);
    let index = inventory
        .members()
        .iter()
        .position(|member| member.path() == &fixture.path)
        .unwrap();
    let operation =
        LocalGitNavigationReadV1::begin(inventory.intent().origin(), Default::default()).unwrap();
    let projection =
        PreparedOriginalTreeNavigationV1::open_member(inventory, index, vec![], &operation)
            .unwrap();
    operation.seal().unwrap();
    projection.install(&mut workspace).unwrap();
    workspace
}

#[test]
fn original_tree_jump_history_back_fresh_file_retains_exact_breadcrumb_source_and_zero_read_directory(
) {
    for format in ["sha1", "sha256"] {
        let fixture = NestedReal::new(format);
        let mut workspace = opened_nested_tree_file(&fixture);
        let projection = workspace.tree_navigation_v1.as_ref().unwrap().clone();
        let before = fixture.real.snapshot();
        let dirty = std::fs::read(fixture.real.root.join("nested/renamed.txt")).unwrap();
        fixture.action(&mut workspace, RowAction::FileProvenance(FileAction::Blame));
        assert!(!projection.visible_for(&workspace));
        assert!(workspace
            .display_rows_v1()
            .iter()
            .any(|row| row.label.contains("Line blame — chosen saved text")));
        assert!(fixture.action(&mut workspace, RowAction::FileProvenance(FileAction::Back)) > 0);
        assert!(Arc::ptr_eq(
            workspace.tree_navigation_v1.as_ref().unwrap(),
            &projection
        ));
        assert!(projection.visible_for(&workspace));
        let fresh = workspace.preview.as_ref().unwrap();
        assert_eq!(
            (&fresh.revision, &fresh.path, &fresh.object_id),
            (
                &fixture.chosen,
                &fixture.path,
                &projection.preview().unwrap().object_id
            )
        );
        assert!(fresh.content.starts_with(b"actual saved line 001\n"));
        assert!(
            !std::ptr::eq(fresh, projection.preview().unwrap()),
            "The old body is not the freshly verified file"
        );
        let folder = projection
            .breadcrumbs()
            .iter()
            .find(|crumb| crumb.label() == "[Folder: nested]")
            .unwrap();
        let operation = LocalGitNavigationReadV1::begin(
            projection.inventory().intent().origin(),
            Default::default(),
        )
        .unwrap();
        fixture.real.clear_trace();
        let directory = projection.navigate(folder.action(), &operation).unwrap();
        assert_eq!(operation.seal().unwrap().children_started(), 0);
        assert!(fixture.real.children().is_empty());
        directory.clone().install(&mut workspace).unwrap();
        assert!(directory.visible_for(&workspace));
        assert!(workspace
            .display_rows_v1()
            .iter()
            .any(|row| row.label.contains("nested/renamed.txt")));
        assert_eq!(fixture.real.snapshot(), before);
        assert_eq!(
            std::fs::read(fixture.real.root.join("nested/renamed.txt")).unwrap(),
            dirty
        );
    }
}

#[test]
fn original_tree_jump_history_back_does_not_revive_foreign_hidden_tree_projection() {
    let fixture = NestedReal::new("sha256");
    let foreign = NestedReal::new("sha256");
    let mut workspace = opened_nested_tree_file(&fixture);
    let foreign_workspace = opened_nested_tree_file(&foreign);
    let foreign_projection = foreign_workspace
        .tree_navigation_v1
        .as_ref()
        .unwrap()
        .clone();
    let before = fixture.real.snapshot();
    fixture.action(&mut workspace, RowAction::FileProvenance(FileAction::Blame));
    workspace.tree_navigation_v1 = Some(foreign_projection.clone());
    assert!(fixture.action(&mut workspace, RowAction::FileProvenance(FileAction::Back)) > 0);
    assert!(workspace.tree_navigation_v1.is_none());
    assert!(!foreign_projection.visible_for(&workspace));
    assert_eq!(workspace.preview.as_ref().unwrap().path, fixture.path);
    assert_eq!(workspace.preview.as_ref().unwrap().revision, fixture.chosen);
    assert_eq!(fixture.real.snapshot(), before);
}
#[test]
fn original_tree_jump_projection_collapse_root_and_cached_breadcrumb_do_not_read_git() {
    let fixture = NestedReal::new("sha1");
    let ticket = jump_ticket(&fixture, "nested");
    assert_eq!(ticket.member().kind(), OriginalTreeKindV1::Directory);
    let op =
        LocalGitNavigationReadV1::begin(ticket.inventory().intent().origin(), Default::default())
            .unwrap();
    let expanded = PreparedOriginalTreeNavigationV1::jump(&ticket, &op).unwrap();
    op.seal().unwrap();
    let action = expanded
        .rows()
        .iter()
        .find_map(|row| match &row.action {
            Some(RowAction::Tree(action @ OriginalTreeNavigationActionV1::Toggle(ticket)))
                if ticket.index
                    == Some(
                        ticket
                            .source
                            .inventory
                            .members()
                            .iter()
                            .position(|member| member.path().0 == b"nested")
                            .unwrap(),
                    ) =>
            {
                Some(action.clone())
            }
            _ => None,
        })
        .unwrap();
    let op =
        LocalGitNavigationReadV1::begin(expanded.inventory().intent().origin(), Default::default())
            .unwrap();
    fixture.real.clear_trace();
    let collapsed = expanded.navigate(&action, &op).unwrap();
    assert!(!collapsed
        .rows()
        .iter()
        .any(|row| row.label.contains("nested/renamed.txt")));
    assert_eq!(op.seal().unwrap().children_started(), 0);
    assert!(fixture.real.children().is_empty());
    let op = LocalGitNavigationReadV1::begin(
        collapsed.inventory().intent().origin(),
        Default::default(),
    )
    .unwrap();
    let root = collapsed
        .navigate(collapsed.breadcrumbs()[0].action(), &op)
        .unwrap();
    assert_eq!(op.seal().unwrap().children_started(), 0);
    assert!(root.rows().iter().any(|row| row.label.contains("nested")));
}
#[test]
fn original_tree_jump_projection_stale_row_ticket_from_previous_view_refuses_before_child() {
    let fixture = NestedReal::new("sha1");
    let inv = inventory(&fixture);
    let op = LocalGitNavigationReadV1::begin(inv.intent().origin(), Default::default()).unwrap();
    let old = PreparedOriginalTreeNavigationV1::root(inv.clone(), &op).unwrap();
    let newer = PreparedOriginalTreeNavigationV1::root(inv, &op).unwrap();
    op.seal().unwrap();
    let old_action = old
        .rows()
        .iter()
        .find_map(|row| match &row.action {
            Some(RowAction::Tree(action)) => Some(action.clone()),
            _ => None,
        })
        .unwrap();
    let op =
        LocalGitNavigationReadV1::begin(newer.inventory().intent().origin(), Default::default())
            .unwrap();
    fixture.real.clear_trace();
    assert!(newer.navigate(&old_action, &op).is_err());
    assert_eq!(op.accounting().unwrap().children_started(), 0);
}
#[test]
fn original_tree_jump_projection_install_retires_legacy_large_bodies_and_preserves_real_roles() {
    let fixture = NestedReal::new("sha256");
    let mut workspace = fixture.workspace();
    let author = workspace.selected_commit.as_ref().unwrap().author.clone();
    let committer = workspace
        .selected_commit
        .as_ref()
        .unwrap()
        .committer
        .clone();
    let inv = inventory_from(&workspace);
    let op = LocalGitNavigationReadV1::begin(inv.intent().origin(), Default::default()).unwrap();
    let prepared = PreparedOriginalTreeNavigationV1::root(inv, &op).unwrap();
    op.seal().unwrap();
    prepared.clone().install(&mut workspace).unwrap();
    assert!(workspace.rows.is_empty() && workspace.tree.is_none() && workspace.preview.is_none());
    assert!(prepared.visible_for(&workspace));
    assert_eq!(workspace.selected_commit.as_ref().unwrap().author, author);
    assert_eq!(
        workspace.selected_commit.as_ref().unwrap().committer,
        committer
    );
    let copied = workspace.clone();
    assert!(Arc::ptr_eq(
        copied.tree_navigation_v1.as_ref().unwrap(),
        &prepared
    ));
}
#[test]
fn original_tree_jump_projection_cannot_shadow_real_actor_history_graph_or_working_frames() {
    let fixture = NestedReal::new("sha1");
    let mut workspace = fixture.workspace();
    let inv = inventory_from(&workspace);
    let op = LocalGitNavigationReadV1::begin(inv.intent().origin(), Default::default()).unwrap();
    let prepared = PreparedOriginalTreeNavigationV1::root(inv, &op).unwrap();
    op.seal().unwrap();
    prepared.clone().install(&mut workspace).unwrap();
    workspace.actor_profile = Some(GitRole::Author);
    assert!(!prepared.visible_for(&workspace));
    workspace.actor_profile = None;
    for mode in [
        RepositoryMode::Graph,
        RepositoryMode::Working,
        RepositoryMode::Staged,
    ] {
        workspace.mode = mode;
        assert!(!prepared.visible_for(&workspace));
    }
    workspace.mode = RepositoryMode::Tree;
    let mut provenance = FileProvenance::new(fixture.chosen.clone(), fixture.path.clone());
    provenance.view = ProvenanceView::Blame;
    workspace.file_provenance = Some(provenance);
    assert!(!prepared.visible_for(&workspace));
}
#[test]
fn original_tree_jump_projection_opaque_route_restore_is_exact_fresh_source_not_serialized_body() {
    let fixture = NestedReal::new("sha256");
    let ticket = jump_ticket(&fixture, "nested/renamed.txt");
    let op =
        LocalGitNavigationReadV1::begin(ticket.inventory().intent().origin(), Default::default())
            .unwrap();
    let projection = PreparedOriginalTreeNavigationV1::jump(&ticket, &op).unwrap();
    op.seal().unwrap();
    let route =
        super::super::tree_jump_route_v1::OriginalTreeSavedRouteV1::capture(&projection).unwrap();
    assert!(route.identity_bytes().unwrap().len() < 16 * 1024);
    let op = LocalGitNavigationReadV1::begin(
        projection.inventory().intent().origin(),
        Default::default(),
    )
    .unwrap();
    fixture.real.clear_trace();
    let restored = route.restore(&op).unwrap();
    assert_eq!(restored.preview().unwrap().revision, fixture.chosen);
    assert_eq!(restored.preview().unwrap().path, fixture.path);
    assert_eq!(
        restored.preview().unwrap().object_id,
        projection.preview().unwrap().object_id
    );
    assert!(op.seal().unwrap().children_started() > 0);
    assert!(!fixture.real.children().is_empty());
}
#[test]
fn original_tree_jump_projection_cancelled_file_selection_has_no_body_or_command_publication() {
    let fixture = NestedReal::new("sha1");
    let ticket = jump_ticket(&fixture, "nested/renamed.txt");
    let token = CancellationToken::default();
    let op = LocalGitNavigationReadV1::begin(ticket.inventory().intent().origin(), token.clone())
        .unwrap();
    token.cancel();
    fixture.real.clear_trace();
    assert!(PreparedOriginalTreeNavigationV1::jump(&ticket, &op).is_err());
    assert!(fixture.real.children().is_empty());
    assert_eq!(op.accounting().unwrap().children_started(), 0);
}

#[test]
fn original_tree_jump_route_capture_uses_actual_prepared_file_member_without_git() {
    for format in ["sha1", "sha256"] {
        let fixture = NestedReal::new(format);
        let ticket = jump_ticket(&fixture, "nested/renamed.txt");
        let op = LocalGitNavigationReadV1::begin(
            ticket.inventory().intent().origin(),
            Default::default(),
        )
        .unwrap();
        let projection = PreparedOriginalTreeNavigationV1::jump(&ticket, &op).unwrap();
        op.seal().unwrap();
        assert_eq!(projection.source.listed.as_ref(), &[ticket.index()]);
        let before = fixture.real.snapshot();
        fixture.real.clear_trace();
        let route =
            super::super::tree_jump_route_v1::OriginalTreeSavedRouteV1::capture(&projection)
                .unwrap();
        let identity = route.identity_bytes().unwrap();
        for field in [
            fixture.path.0.as_slice(),
            ticket.member().object_id().as_bytes(),
            ticket.member().mode().as_bytes(),
        ] {
            assert!(identity.windows(field.len()).any(|part| part == field));
        }
        assert!(fixture.real.children().is_empty());
        assert_eq!(fixture.real.snapshot(), before);
    }
}

#[test]
fn original_tree_jump_route_inconsistent_prepared_file_member_refuses_without_lookup_fallback() {
    for wrong_member_shape in [false, true] {
        let fixture = NestedReal::new("sha256");
        let ticket = jump_ticket(&fixture, "nested/renamed.txt");
        let op = LocalGitNavigationReadV1::begin(
            ticket.inventory().intent().origin(),
            Default::default(),
        )
        .unwrap();
        let projection = PreparedOriginalTreeNavigationV1::jump(&ticket, &op).unwrap();
        op.seal().unwrap();
        let mut projection = Arc::try_unwrap(projection).unwrap();
        if wrong_member_shape {
            projection.source = Arc::new(OriginalTreeRowSourceV1 {
                inventory: projection.source.inventory.clone(),
                expanded: projection.source.expanded.clone(),
                listed: vec![ticket.index(), ticket.index()].into(),
            });
        } else {
            projection.path = Some(RawPath(
                b"same display is not the original raw path".to_vec(),
            ));
        }
        let before = fixture.real.snapshot();
        fixture.real.clear_trace();
        assert!(
            super::super::tree_jump_route_v1::OriginalTreeSavedRouteV1::capture(&projection)
                .is_err()
        );
        assert!(fixture.real.children().is_empty());
        assert_eq!(fixture.real.snapshot(), before);
    }
}
