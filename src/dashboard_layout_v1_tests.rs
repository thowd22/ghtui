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
fn design_footer_v90_dashboard_layout_keys_click_and_press_the_same_action_after_resize() {
    for (width, height) in [(140, 42), (80, 24)] {
        let mut workspace = prepared();
        workspace.apply(MetricsWorkspaceAction::Layout).unwrap();
        let theme = crate::theme::Theme::new(
            crate::config::ThemeName::CatppuccinMocha,
            crate::theme::ColorMode::NoColor,
        );
        let controls = metrics_ui::footer_keys(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut rendered = None;
        let mut targets = Vec::new();
        terminal
            .draw(|frame| {
                rendered = Some(metrics_ui::draw(
                    frame,
                    &mut workspace,
                    Rect::new(0, 0, width, height - 1),
                    theme,
                    &Default::default(),
                    true,
                ));
                targets = crate::footer::render(
                    frame,
                    Rect::new(0, height - 1, width, 1),
                    theme,
                    &controls,
                    None,
                );
            })
            .unwrap();
        let rendered = rendered.unwrap();
        // Layout editing keys stay painted at 80 columns; the rest reach by key (and palette).
        let painted = [' ', '<', '>', 'L'];
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
            let expected = crate::app::Action::Metrics(action);
            assert_eq!(
                crate::footer::resolve_key(&controls, KeyEvent::new(key, KeyModifiers::NONE)),
                Some(expected.clone())
            );
            let target = targets.iter().find(|(_, action)| *action == expected);
            let KeyCode::Char(character) = key else {
                unreachable!()
            };
            if width >= 140 || painted.contains(&character) {
                let (rect, _) = target.unwrap_or_else(|| panic!("{key:?} painted at {width}"));
                assert!(rect.right() <= width && rect.y == height - 1);
            }
        }
        // The static hint and the old button rows are gone; rows start at the top.
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!text.contains("Tab buttons") && !text.contains("[L Layout]"));
        assert!(text.contains("Layout only"));
        assert!(rendered.visible_rows > 0);
        assert!(rendered.hits.iter().all(|hit| hit.rect.y < height - 1));
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
    let controls = metrics_ui::footer_keys(&workspace);
    assert!(crate::footer::layout(&controls, 140).is_empty());
    for key in ['O', ' ', '<', 'S', 'L'] {
        assert!(matches!(
            crate::footer::resolve_key(
                &controls,
                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)
            ),
            Some(crate::app::Action::Explain(_))
        ));
    }
    assert_eq!(workspace.layout, original);
}
