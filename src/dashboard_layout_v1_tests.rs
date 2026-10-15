use super::*;

#[test]
fn dashboards_reordered_hidden_cards_inspect_and_open_original_kind_not_visible_index() {
    let mut workspace = prepared();
    let snapshot = workspace.snapshot.clone();
    let evidence = workspace.evidence.clone();
    workspace.apply(MetricsWorkspaceAction::Layout).unwrap();
    workspace
        .apply(MetricsWorkspaceAction::ToggleWidget(0))
        .unwrap();
    workspace.apply(MetricsWorkspaceAction::Select(1)).unwrap();
    workspace
        .apply(MetricsWorkspaceAction::MoveWidget(false))
        .unwrap();
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert_eq!(workspace.rows().len(), 7);
    assert!(workspace.rows()[0].label.starts_with("Needs my review:"));
    workspace.apply(MetricsWorkspaceAction::Inspect(0)).unwrap();
    assert_eq!(workspace.view, MetricsView::CardInfo(1));
    assert!(workspace
        .rows()
        .iter()
        .any(|row| row.label.contains("non-draft PRs")));
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert!(enter(&mut workspace).is_none());
    assert_eq!(workspace.view, MetricsView::Contributors(1));
    let Some(MetricsAction::Open(MetricTarget::PullRequest(target))) = enter(&mut workspace) else {
        panic!("Expected original exact PR target")
    };
    assert_eq!(
        (
            target.repository.as_str(),
            target.number,
            target.database_id
        ),
        ("example/a", 7, Some(107))
    );
    assert!(Arc::ptr_eq(&workspace.snapshot, &snapshot));
    assert!(Arc::ptr_eq(&workspace.evidence, &evidence));
}

#[test]
fn dashboards_layout_return_clamps_hidden_cards_and_never_hides_last_widget() {
    let mut workspace = prepared();
    workspace.selected = 7;
    workspace.top = 7;
    workspace.apply(MetricsWorkspaceAction::Layout).unwrap();
    for index in 0..7 {
        workspace
            .apply(MetricsWorkspaceAction::ToggleWidget(index))
            .unwrap();
    }
    let before = workspace.layout.clone();
    assert!(workspace
        .apply(MetricsWorkspaceAction::ToggleWidget(7))
        .unwrap_err()
        .contains("at least one"));
    assert_eq!(workspace.layout, before);
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert_eq!((workspace.selected, workspace.top), (0, 0));
    assert_eq!(workspace.rows().len(), 1);
    assert!(workspace.rows()[0]
        .label
        .starts_with("Latest observed workflow"));
    assert!(workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Card(
            MetricKind::OpenPrs
        )))
        .is_err());
}

#[test]
fn dashboards_layout_and_persistence_buttons_share_mouse_keyboard_semantics_after_resize() {
    for (width, height) in [(120, 38), (80, 24)] {
        let mut workspace = prepared();
        workspace.apply(MetricsWorkspaceAction::Layout).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut rendered = None;
        terminal
            .draw(|frame| {
                rendered = Some(metrics_ui::draw(
                    frame,
                    &mut workspace,
                    Rect::new(0, 0, width, height),
                    crate::theme::Theme::new(
                        crate::config::ThemeName::Dark,
                        crate::theme::ColorMode::NoColor,
                    ),
                    &Default::default(),
                ));
            })
            .unwrap();
        let rendered = rendered.unwrap();
        for (key, action) in [
            (KeyCode::Char('L'), MetricsWorkspaceAction::Layout),
            (
                KeyCode::Char('S'),
                MetricsWorkspaceAction::Metric(MetricsAction::SaveDashboard),
            ),
            (
                KeyCode::Char('O'),
                MetricsWorkspaceAction::Metric(MetricsAction::RestoreDashboard),
            ),
            (
                KeyCode::Char('D'),
                MetricsWorkspaceAction::Metric(MetricsAction::DefaultDashboard),
            ),
            (KeyCode::Char(' '), MetricsWorkspaceAction::ToggleWidget(0)),
            (
                KeyCode::Char('<'),
                MetricsWorkspaceAction::MoveWidget(false),
            ),
            (KeyCode::Char('>'), MetricsWorkspaceAction::MoveWidget(true)),
        ] {
            let hit = rendered
                .hits
                .iter()
                .find(|hit| hit.action == action)
                .unwrap();
            assert_eq!(
                metrics_ui::keyboard_action(KeyEvent::new(key, KeyModifiers::NONE), &workspace),
                Some(action.clone())
            );
            assert_eq!(
                metrics_ui::mouse_action(
                    Event::Mouse(MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: hit.rect.x,
                        row: hit.rect.y,
                        modifiers: KeyModifiers::NONE
                    }),
                    &workspace,
                    &rendered
                ),
                Some(action)
            );
        }
        assert!(rendered.visible_rows > 0);
    }
}

#[test]
fn dashboards_pending_layout_clicks_and_keys_cannot_edit_or_restore_stale_widgets() {
    let mut workspace = prepared();
    workspace.apply(MetricsWorkspaceAction::Layout).unwrap();
    let original = workspace.layout.clone();
    assert_eq!(
        workspace
            .apply(MetricsWorkspaceAction::Metric(MetricsAction::SaveDashboard))
            .unwrap(),
        Some(MetricsAction::SaveDashboard)
    );
    assert!(workspace.rows().is_empty());
    assert!(workspace
        .apply(MetricsWorkspaceAction::ToggleWidget(0))
        .is_err());
    assert!(matches!(
        metrics_ui::keyboard_action(
            KeyEvent::new(KeyCode::Char('O'), KeyModifiers::NONE),
            &workspace
        ),
        Some(MetricsWorkspaceAction::Explain(_))
    ));
    assert_eq!(workspace.layout, original);
}
