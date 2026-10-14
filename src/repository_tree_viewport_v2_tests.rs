use super::*;
use crate::{
    repository_tree_jump_v1::{OriginalTreeFrameV1, OriginalTreeJumpActionV1 as JumpAction},
    repository_tree_viewport_v2_test_support::{deep, ready},
};
fn operation(jump: &OriginalTreeJumpV1) -> LocalGitNavigationReadV1 {
    LocalGitNavigationReadV1::begin(
        jump.frame().unwrap().inventory().intent().origin(),
        Default::default(),
    )
    .unwrap()
}
#[test]
fn original_tree_viewport_generic_notice_owns_actual_paint_key_and_mouse_without_closing_retained_folder(
) {
    use crate::app::{Action, App, Overlay, View};
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::{backend::TestBackend, Terminal};
    for mouse in [false, true] {
        let (real, workspace, jump, projection) = deep("sha256");
        let intent =
            TreeFolderIntentV2::capture(projection.clone(), projection.breadcrumbs().len() - 1)
                .unwrap();
        let op = operation(&jump);
        let folder = intent.prepare(38, TreeDetailFieldV2::Path, &op).unwrap();
        assert_eq!(op.seal().unwrap().children_started(), 0);
        let mut app = App {
            view: View::Repository,
            workspace: Some(workspace),
            original_tree_jump_v1: Some(jump),
            refresh_requested: false,
            ..Default::default()
        };
        app.original_tree_viewport_v2.width = 40;
        app.original_tree_viewport_v2.folder_intent = Some(intent.clone());
        app.original_tree_viewport_v2.folder = Some(folder.clone());
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Saved folder • original detail"));
        app.dispatch(Action::Explain(
            "GENUINE existing generic notice owns input first".into(),
        ));
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("GENUINE existing generic notice"));
        assert!(!text.contains("Saved folder • original detail"));
        assert_eq!(app.overlay, Some(Overlay::Notice));
        if mouse {
            let close = app.hits.last().unwrap().rect;
            app.handle_event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: close.x,
                row: close.y,
                modifiers: KeyModifiers::NONE,
            }));
        } else {
            app.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        }
        assert_eq!(app.overlay, None);
        assert!(Arc::ptr_eq(
            app.original_tree_viewport_v2
                .folder_intent
                .as_ref()
                .unwrap(),
            &intent
        ));
        assert!(Arc::ptr_eq(
            app.original_tree_viewport_v2.folder.as_ref().unwrap(),
            &folder
        ));
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Saved folder • original detail"));
        assert!(real.children().is_empty());
    }
}

#[test]
fn original_tree_viewport_deep_genuine_source_reserves_body_exact_overflow_all_widths() {
    for format in ["sha1", "sha256"] {
        let (_real, workspace, jump, projection) = deep(format);
        let op = operation(&jump);
        let strip = TreeBreadcrumbStripV2::prepare(projection.clone(), &op).unwrap();
        assert!(projection.breadcrumbs().len() > 20);
        for (width, height) in [(120, 20), (80, 12), (40, 8), (40, 4), (20, 1), (0, 0)] {
            let plan = strip.layout(Rect::new(2, 3, width, height));
            assert!(plan.used_rows <= 3);
            assert!(plan.body_rows >= height.min(4));
            assert_eq!(plan.body_rows + plan.used_rows, height);
            for cell in &plan.cells {
                assert!(cell.rect.right() <= 2 + width);
                assert!(cell.rect.bottom() <= 3 + height);
            }
            if height > 4 && width > 0 {
                assert!(plan.omitted > 0);
                assert!(plan
                    .cells
                    .iter()
                    .any(|cell| cell.target == TreeCrumbTargetV2::Commands));
                for cell in plan
                    .cells
                    .iter()
                    .filter(|cell| cell.original_index.is_some())
                {
                    assert_eq!(
                        cell.target,
                        TreeCrumbTargetV2::Directory(
                            projection.breadcrumbs()[cell.original_index.unwrap()]
                                .action()
                                .clone()
                        )
                    );
                }
            }
        }
        // Row count is not enough: even a two-item genuine chain can have a
        // clipped label. Its exact typed palette access must remain visible.
        let shallow = projection
            .navigate(projection.breadcrumbs()[1].action(), &op)
            .unwrap();
        assert_eq!(shallow.breadcrumbs().len(), 2);
        let shallow_strip = TreeBreadcrumbStripV2::prepare(shallow, &op).unwrap();
        let narrow = shallow_strip.layout(Rect::new(0, 0, 20, 12));
        let access = narrow
            .cells
            .iter()
            .find(|cell| cell.target == TreeCrumbTargetV2::Commands)
            .expect("Clipped shallow path requires explicit actual palette access");
        assert!(crate::editor::display_width(shallow_strip.label(access)) <= 20);
        assert!(shallow_strip.label(access).contains('2'));
        assert!(strip.source().visible_for(&workspace));
        assert_eq!(op.seal().unwrap().children_started(), 0);
    }
}
#[test]
fn original_tree_viewport_genuine_full_sha_path_and_hex_wrap_at40_without_lost_suffix() {
    let (real, _workspace, jump) = ready(
        "sha256",
        &[b"same-\xff-really-long-original-raw-path-with-a-readable-suffix.rs"],
    );
    let before = real.snapshot();
    let mut cache = TreeDetailCacheV2::default();
    for field in TreeDetailFieldV2::ALL {
        let request = cache.request(&jump, 38, field).unwrap().unwrap();
        let raw = request.ticket().member().path().0.clone();
        let sha = request.ticket().inventory().intent().sha().to_owned();
        let display = request.ticket().member().display().to_owned();
        let op = operation(&jump);
        let detail = PreparedTreeDetailV2::prepare(request, &op).unwrap();
        assert!(detail.header().concat().contains(&sha));
        assert!(detail
            .rows()
            .iter()
            .all(|row| crate::editor::display_width(row) <= 38));
        let all = detail.rows().concat();
        match field {
            TreeDetailFieldV2::Path => assert!(all.contains(&display)),
            TreeDetailFieldV2::Bytes => assert!(all.contains(
                &raw.iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            )),
            TreeDetailFieldV2::Saved => assert!(all.contains(&sha)),
            TreeDetailFieldV2::Object => {
                assert!(all.contains(detail.request().ticket().member().object_id()))
            }
        }
        assert!(cache.admit(detail, &jump));
        assert!(cache.ready(&jump, 38, field).is_some());
        assert_eq!(op.seal().unwrap().children_started(), 0);
    }
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
#[test]
fn original_tree_viewport_superseded_width_and_field_request_refuses_late_detail() {
    let (real, _workspace, jump) = ready("sha1", &[b"actual.rs"]);
    let mut cache = TreeDetailCacheV2::default();
    let old = cache
        .request(&jump, 78, TreeDetailFieldV2::Path)
        .unwrap()
        .unwrap();
    let new = cache
        .request(&jump, 38, TreeDetailFieldV2::Saved)
        .unwrap()
        .unwrap();
    let op = operation(&jump);
    let late = PreparedTreeDetailV2::prepare(old, &op).unwrap();
    assert!(!cache.admit(late, &jump));
    assert!(Arc::ptr_eq(cache.pending().unwrap(), &new));
    let current = PreparedTreeDetailV2::prepare(new, &op).unwrap();
    assert!(cache.admit(current, &jump));
    assert!(cache.ready(&jump, 78, TreeDetailFieldV2::Path).is_none());
    assert_eq!(op.seal().unwrap().children_started(), 0);
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_viewport_same_member_in_new_query_cannot_reuse_original_detail_ticket() {
    let (real, _workspace, mut jump) = ready("sha1", &[b"same-original.rs"]);
    let mut cache = TreeDetailCacheV2::default();
    let old = cache
        .request(&jump, 38, TreeDetailFieldV2::Path)
        .unwrap()
        .unwrap();
    let op = operation(&jump);
    let old_detail = PreparedTreeDetailV2::prepare(old, &op).unwrap();
    op.seal().unwrap();
    jump.apply(JumpAction::Insert("same".into())).unwrap();
    let request = jump.pending().unwrap().clone();
    let op =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let frame = OriginalTreeFrameV1::prepare(request, &op).unwrap();
    op.seal().unwrap();
    assert!(jump.accept(frame));
    assert!(!cache.admit(old_detail, &jump));
    assert!(cache.ready(&jump, 38, TreeDetailFieldV2::Path).is_none());
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_viewport_bounded_cache_reuses_only_exact_current_frame_width_field() {
    let (real, _workspace, jump) = ready("sha1", &[b"actual.rs"]);
    let mut cache = TreeDetailCacheV2::default();
    let op = operation(&jump);
    for width in [78, 38] {
        let request = cache
            .request(&jump, width, TreeDetailFieldV2::Path)
            .unwrap()
            .unwrap();
        let detail = PreparedTreeDetailV2::prepare(request, &op).unwrap();
        assert!(cache.admit(detail, &jump));
    }
    assert!(cache
        .request(&jump, 78, TreeDetailFieldV2::Path)
        .unwrap()
        .is_none());
    assert!(cache.ready(&jump, 78, TreeDetailFieldV2::Path).is_some());
    assert!(cache
        .request(&jump, 38, TreeDetailFieldV2::Saved)
        .unwrap()
        .is_some());
    assert!(cache.ready(&jump, 78, TreeDetailFieldV2::Path).is_none());
    cache.clear();
    assert!(cache.pending().is_none());
    assert_eq!(op.seal().unwrap().children_started(), 0);
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_viewport_canceled_completed_and_tiny_width_refuse_before_metadata_prepare() {
    let (_real, _workspace, jump) = ready("sha1", &[b"actual.rs"]);
    let mut cache = TreeDetailCacheV2::default();
    assert!(cache.request(&jump, 19, TreeDetailFieldV2::Path).is_err());
    assert!(cache.request(&jump, 401, TreeDetailFieldV2::Path).is_err());
    let request = cache
        .request(&jump, 38, TreeDetailFieldV2::Path)
        .unwrap()
        .unwrap();
    let op = operation(&jump);
    op.cancellation().cancel();
    assert!(PreparedTreeDetailV2::prepare(request.clone(), &op).is_err());
    assert_eq!(op.accounting().unwrap().children_started(), 0);
    let completed = operation(&jump);
    assert_eq!(completed.seal().unwrap().children_started(), 0);
    assert!(PreparedTreeDetailV2::prepare(request, &completed).is_err());
    assert_eq!(completed.accounting().unwrap().children_started(), 0);
}
#[test]
fn original_tree_viewport_fields_have_keyboard_mouse_equivalent_typed_order() {
    let (_real, _workspace, jump) = ready("sha1", &[b"actual.rs"]);
    let op = operation(&jump);
    let mut field = TreeDetailFieldV2::Path;
    for expected in [
        TreeDetailFieldV2::Bytes,
        TreeDetailFieldV2::Saved,
        TreeDetailFieldV2::Object,
        TreeDetailFieldV2::Path,
    ] {
        assert_eq!(field.next(false), expected);
        assert_eq!(expected.next(true), field);
        let action = TreeViewportActionV2::DetailField(expected);
        assert_eq!(action, TreeViewportActionV2::DetailField(field.next(false)));
        field = expected;
    }
    assert_eq!(op.seal().unwrap().children_started(), 0);
}
#[test]
fn original_tree_viewport_shared_focus_keeps_no_color_selection_and_semantic_role() {
    for settings in 0..3 {
        let theme = Theme::from_settings(settings, true);
        let baseline = theme.selected(SemanticToken::Added);
        let focused = focused_style_v2(theme, SemanticToken::Added, true, true);
        assert_eq!(focused.fg, baseline.fg);
        assert_eq!(focused.bg, baseline.bg);
        assert!(focused
            .add_modifier
            .contains(Modifier::BOLD | Modifier::REVERSED | Modifier::UNDERLINED));
        assert_eq!(
            focused_style_v2(theme, SemanticToken::Reference, false, false),
            theme.style(SemanticToken::Reference)
        );
    }
}
#[test]
fn original_tree_viewport_folder_detail_is_exact_genuine_node_not_new_projection() {
    let (real, mut workspace, jump, projection) = deep("sha256");
    let before = real.snapshot();
    let op = operation(&jump);
    let intent =
        TreeFolderIntentV2::capture(projection.clone(), projection.breadcrumbs().len() - 1)
            .unwrap();
    assert!(intent.is_current(&workspace));
    let detail = intent.prepare(38, TreeDetailFieldV2::Path, &op).unwrap();
    let OriginalTreeNavigationActionV1::Directory(ticket) = intent.action() else {
        panic!("Actual original directory required");
    };
    assert!(detail
        .rows()
        .concat()
        .contains(ticket.member().unwrap().display()));
    assert!(detail.rows().concat().contains(ticket.saved_sha()));
    workspace.tree_navigation_v1 = None;
    assert!(!intent.is_current(&workspace));
    assert_eq!(op.seal().unwrap().children_started(), 0);
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
