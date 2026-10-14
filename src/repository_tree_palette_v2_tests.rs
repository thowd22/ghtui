use super::*;
use crate::{
    command_palette::{CommandPalette, PaletteAction, PaletteOutcome},
    navigation_catalogue::PreparedCatalogue,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_tree_jump_v1::OriginalTreeFrameV1,
    repository_tree_viewport_v2_test_support::{deep, ready},
};
#[test]
fn original_tree_viewport_palette_finite_source_controls_preserve_original_global_first6() {
    let (real, workspace, jump) = ready("sha1", &[b"actual.rs"]);
    let app = App {
        view: View::Repository,
        workspace: Some(workspace),
        original_tree_jump_v1: Some(jump),
        ..App::default()
    };
    let prepared = PreparedCatalogue::prepare(&app, 1).unwrap();
    assert_eq!(
        prepared
            .entries
            .iter()
            .take(6)
            .map(|entry| entry.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Open Inbox",
            "Open Repository",
            "Open Search",
            "Open Actions",
            "Open Dashboard",
            "Inspect capabilities"
        ]
    );
    let entries = entries_v2(&app, None).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries[0].action,
        Action::OriginalTreeV1(OriginalTreeJumpActionV1::Open)
    );
    assert!(
        matches!(&entries[1].action,Action::Explain(message) if message.contains("No further loaded"))
    );
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_viewport_palette_next_keyboard_and_mouse_emit_exact_existing_page_action() {
    let paths = (0..150)
        .map(|index| format!("file-{index:03}").into_bytes())
        .collect::<Vec<_>>();
    let refs = paths.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let (real, workspace, jump) = ready("sha256", &refs);
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        original_tree_jump_v1: Some(jump),
        ..App::default()
    };
    let entries = entries_v2(&app, None).unwrap();
    assert_eq!(
        entries[1].action,
        Action::OriginalTreeV1(OriginalTreeJumpActionV1::Page(1))
    );
    for mouse in [false, true] {
        let mut palette = CommandPalette::new(entries_v2(&app, None).unwrap()).unwrap();
        palette.dispatch(PaletteAction::Paste("Next100 original saved paths".into()));
        let outcome = palette.dispatch(if mouse {
            PaletteAction::Activate(1)
        } else {
            PaletteAction::Choose
        });
        assert!(matches!(
            outcome,
            PaletteOutcome::Chosen {
                original_index: 1,
                action: Action::OriginalTreeV1(OriginalTreeJumpActionV1::Page(1))
            }
        ));
    }
    app.original_tree_jump_v1
        .as_mut()
        .unwrap()
        .apply(OriginalTreeJumpActionV1::Page(1))
        .unwrap();
    let pending = app
        .original_tree_jump_v1
        .as_ref()
        .unwrap()
        .pending()
        .unwrap()
        .clone();
    let op =
        LocalGitNavigationReadV1::begin(pending.intent().origin(), Default::default()).unwrap();
    let frame = OriginalTreeFrameV1::prepare(pending, &op).unwrap();
    op.seal().unwrap();
    assert!(app.original_tree_jump_v1.as_mut().unwrap().accept(frame));
    assert!(matches!(
        &entries_v2(&app, None).unwrap()[1].action,
        Action::Explain(_)
    ));
    assert_eq!(
        entries_v2(&app, None).unwrap()[2].action,
        Action::OriginalTreeV1(OriginalTreeJumpActionV1::Page(-1))
    );
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_viewport_palette_genuine_folders_keep_source_action_and_exact_detail_intent() {
    let (real, workspace, jump, projection) = deep("sha1");
    let app = App {
        view: View::Repository,
        workspace: Some(workspace),
        original_tree_jump_v1: Some(jump),
        ..App::default()
    };
    let entries = entries_v2(&app, None).unwrap();
    assert_eq!(entries.len(), 3 + projection.breadcrumbs().len() * 2);
    assert!(entries.len() <= 134);
    for (index, crumb) in projection.breadcrumbs().iter().enumerate() {
        let open = entries
            .iter()
            .find(|entry| entry.id == format!("repository.tree.folder.{index}"))
            .unwrap();
        assert_eq!(
            open.action,
            Action::RepositoryRow(RowAction::Tree(crumb.action().clone()))
        );
        let inspect = entries
            .iter()
            .find(|entry| entry.id == format!("repository.tree.folder-detail.{index}"))
            .unwrap();
        let Action::OriginalTreeViewportV2(TreeViewportActionV2::InspectFolder(intent)) =
            &inspect.action
        else {
            panic!("Genuine original folder required")
        };
        assert!(Arc::ptr_eq(intent.source(), &projection));
        assert_eq!(intent.action(), crumb.action());
    }
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_viewport_palette_unavailable_busy_foreign_surface_is_honest_explanation() {
    let mut app = App {
        view: View::Repository,
        ..App::default()
    };
    assert!(entries_v2(&app, None)
        .unwrap()
        .iter()
        .all(|entry| matches!(entry.action, Action::Explain(_))));
    let (_real, workspace, jump) = ready("sha1", &[b"actual.rs"]);
    app.workspace = Some(workspace);
    app.original_tree_jump_v1 = Some(jump);
    app.loading[View::Repository.index()] = true;
    assert!(entries_v2(&app, None)
        .unwrap()
        .iter()
        .all(|entry| matches!(entry.action, Action::Explain(_))));
    app.view = View::Search;
    assert!(entries_v2(&app, None).unwrap().is_empty());
}

#[test]
fn original_tree_viewport_palette_deep_shared_prefix_keeps_original_leaf_searchable_not_label_target(
) {
    let (real, workspace, jump, projection) = deep("sha256");
    let app = App {
        view: View::Repository,
        workspace: Some(workspace),
        original_tree_jump_v1: Some(jump),
        ..App::default()
    };
    let entries = entries_v2(&app, None).unwrap();
    let id = format!(
        "repository.tree.folder-detail.{}",
        projection.breadcrumbs().len() - 1
    );
    let original = entries.iter().position(|entry| entry.id == id).unwrap();
    assert!(entries[original].label.contains("deep-folder-23"));
    assert!(entries[original].label.contains("[name clipped]"));
    let mut palette = CommandPalette::new(entries).unwrap();
    palette.dispatch(PaletteAction::Paste(
        "Inspect exact folder deep-folder-23".into(),
    ));
    assert!(palette.visible_indices().contains(&original));
    // Fuzzy matching legitimately admits other path prefixes; this contract
    // is discovery/member identity, not an unmeasured ranking expectation.
    palette.dispatch(PaletteAction::IndexedRow(original));
    let PaletteOutcome::Chosen {
        original_index,
        action: Action::OriginalTreeViewportV2(TreeViewportActionV2::InspectFolder(intent)),
    } = palette.dispatch(PaletteAction::Choose)
    else {
        panic!("Genuine original deep folder must remain discoverable");
    };
    assert_eq!(original_index, original);
    assert!(Arc::ptr_eq(intent.source(), &projection));
    assert_eq!(
        intent.action(),
        projection.breadcrumbs().last().unwrap().action()
    );
    assert!(real.children().is_empty());
}
