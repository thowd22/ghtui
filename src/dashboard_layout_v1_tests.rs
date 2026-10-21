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
    // Seven metric cards plus the Stale PRs and Unresolved threads widgets.
    assert_eq!(workspace.rows().len(), 9);
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
    // Hide every slot but the workflow card, the Stale PRs and Unresolved threads widgets too.
    for index in (0..10).filter(|index| *index != 7) {
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

fn row_value(workspace: &MetricsWorkspace, title: &str) -> Option<String> {
    workspace
        .card_rows
        .iter()
        .find(|row| row.label.starts_with(&format!("{title}: ")))
        .map(|row| row.label[title.len() + 2..].to_owned())
}
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn dashboards_attention_widgets_are_layout_slots_that_open_their_views() {
    let mut workspace = prepared();
    assert_eq!(workspace.rows().len(), 10);
    let stale = workspace
        .rows()
        .iter()
        .position(|row| row.action == Some(MetricsWorkspaceAction::Stale))
        .unwrap();
    let threads = workspace
        .rows()
        .iter()
        .position(|row| row.action == Some(MetricsWorkspaceAction::Metric(MetricsAction::Threads)))
        .unwrap();
    assert_eq!((stale, threads), (8, 9));
    assert_eq!(
        row_value(&workspace, "Stale PRs").unwrap(),
        format!("{} idle 7+ days", workspace.stale_rows.len())
    );
    assert_eq!(
        row_value(&workspace, "Unresolved threads").unwrap(),
        "Not read • Enter reads current threads"
    );
    // A widget row has no formula or trend; Enter opens it.
    assert!(workspace
        .apply(MetricsWorkspaceAction::Inspect(stale))
        .unwrap_err()
        .contains("Enter opens them"));
    assert!(workspace
        .apply(MetricsWorkspaceAction::Trend(stale))
        .unwrap_err()
        .contains("metric cards"));
    workspace
        .apply(MetricsWorkspaceAction::Select(stale))
        .unwrap();
    assert!(enter(&mut workspace).is_none());
    assert_eq!(workspace.view, MetricsView::Stale);
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert_eq!(
        (workspace.view, workspace.selected),
        (MetricsView::Cards, stale)
    );
    // Move Unresolved threads first and hide Stale PRs, like any metric card.
    workspace.apply(MetricsWorkspaceAction::Layout).unwrap();
    workspace.apply(MetricsWorkspaceAction::Select(9)).unwrap();
    for _ in 0..9 {
        workspace
            .apply(MetricsWorkspaceAction::MoveWidget(false))
            .unwrap();
    }
    assert_eq!(workspace.selected, 0);
    workspace
        .apply(MetricsWorkspaceAction::ToggleWidget(9))
        .unwrap();
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert!(workspace.rows()[0].label.starts_with("Unresolved threads:"));
    assert!(row_value(&workspace, "Stale PRs").is_none());
    workspace.apply(MetricsWorkspaceAction::Select(0)).unwrap();
    assert_eq!(enter(&mut workspace), Some(MetricsAction::Threads));
    // The read's result shows on the card.
    workspace.pending = None;
    workspace.attach_threads(Vec::new(), "0 unresolved".into());
    assert_eq!(
        row_value(&workspace, "Unresolved threads").unwrap(),
        "0 listed • Enter reads again"
    );
}

#[test]
fn dashboards_stale_threshold_cycles_in_layout_and_recomputes_the_widget() {
    let mut workspace = prepared();
    workspace.apply(MetricsWorkspaceAction::Layout).unwrap();
    let last = workspace.rows().len() - 1;
    assert_eq!(last, 10);
    assert!(workspace.rows()[last]
        .label
        .starts_with("Stale threshold: 7 days"));
    // Space (the footer key) and a click act on the same row action.
    workspace
        .apply(MetricsWorkspaceAction::Select(last))
        .unwrap();
    assert_eq!(
        crate::footer::resolve_key(
            &metrics_ui::footer_keys(&workspace),
            key(KeyCode::Char(' '))
        ),
        Some(crate::app::Action::Metrics(
            MetricsWorkspaceAction::ToggleWidget(10)
        ))
    );
    assert_eq!(
        workspace.rows()[last].action,
        Some(MetricsWorkspaceAction::ToggleWidget(10))
    );
    assert!(workspace
        .apply(MetricsWorkspaceAction::MoveWidget(false))
        .unwrap_err()
        .contains("stays last"));
    for expected in [14, 30, 3, 7, 14] {
        workspace
            .apply(MetricsWorkspaceAction::ToggleWidget(10))
            .unwrap();
        assert_eq!(workspace.stale_days, expected);
        assert!(workspace.rows()[last]
            .label
            .starts_with(&format!("Stale threshold: {expected} days")));
        assert!(workspace
            .stale_summary
            .contains(&format!("not updated for {expected}+ days")));
        assert_eq!(
            row_value(&workspace, "Stale PRs").unwrap(),
            format!("{} idle {expected}+ days", workspace.stale_rows.len())
        );
    }
    assert!(workspace.apply_stale_days(5).is_err());
    assert_eq!(workspace.stale_days, 14);
}

#[test]
fn dashboards_named_list_new_rename_delete_by_key_and_click_produce_worker_requests() {
    use crate::dashboard_profile_v1::DashboardEntry;
    let mut workspace = prepared();
    let controls = metrics_ui::footer_keys(&workspace);
    assert_eq!(
        crate::footer::resolve_key(&controls, key(KeyCode::Char('N'))),
        Some(crate::app::Action::Metrics(
            MetricsWorkspaceAction::Dashboards
        ))
    );
    // Lowest priority: N leaves the bar before z and u do.
    let priority = |label: &str| {
        controls
            .iter()
            .find(|control| control.label == label)
            .unwrap()
            .priority
    };
    assert!(priority("Dashboards") > priority("Unresolved threads"));
    workspace.apply(MetricsWorkspaceAction::Dashboards).unwrap();
    assert!(workspace.rows().is_empty());
    let controls = metrics_ui::footer_keys(&workspace);
    for label in ["Rename", "Delete"] {
        assert!(controls
            .iter()
            .any(|control| control.label == label && control.disabled.is_some()));
    }
    assert!(workspace
        .apply(MetricsWorkspaceAction::RenameDashboard)
        .is_err());
    workspace.attach_saved_dashboards(Ok(None));
    workspace.saved_dashboards = Arc::from(vec![
        DashboardEntry {
            name: "Team".into(),
            active: false,
            repositories: 1,
            window_days: 30,
            stale_days: 14,
        },
        DashboardEntry {
            name: "Ops".into(),
            active: true,
            repositories: 2,
            window_days: 7,
            stale_days: 7,
        },
    ]);
    workspace.rebuild_dashboard_rows();
    assert_eq!(workspace.rows().len(), 2);
    assert!(workspace.rows()[1]
        .label
        .starts_with("Ops • active • 2 repositories"));
    // Enter or a click on a row switches to that saved dashboard.
    assert_eq!(
        workspace.rows()[0].action,
        Some(MetricsWorkspaceAction::Metric(
            MetricsAction::SwitchDashboard("Team".into())
        ))
    );
    // New: typed name, Backspace, Enter. Shortcut letters and digits are text here.
    assert_eq!(
        crate::footer::resolve_key(
            &metrics_ui::footer_keys(&workspace),
            key(KeyCode::Char('n'))
        ),
        Some(crate::app::Action::Metrics(
            MetricsWorkspaceAction::NewDashboard
        ))
    );
    workspace
        .apply(MetricsWorkspaceAction::NewDashboard)
        .unwrap();
    for code in [
        KeyCode::Char('Q'),
        KeyCode::Char('q'),
        KeyCode::Char('5'),
        KeyCode::Backspace,
        KeyCode::Char('A'),
    ] {
        let action = metrics_ui::name_entry_action(key(code), &workspace).unwrap();
        workspace.apply(action).unwrap();
    }
    assert_eq!(workspace.name_entry.as_ref().unwrap().text, "QqA");
    // While naming, only Save name and Discard name are on the bar.
    let controls = metrics_ui::footer_keys(&workspace);
    assert_eq!(
        controls
            .iter()
            .map(|control| control.label.as_str())
            .collect::<Vec<_>>(),
        ["Save name", "Discard name"]
    );
    let submit = metrics_ui::name_entry_action(key(KeyCode::Enter), &workspace).unwrap();
    assert_eq!(
        controls[0].action,
        crate::app::Action::Metrics(submit.clone())
    );
    assert_eq!(
        workspace.apply(submit).unwrap(),
        Some(MetricsAction::SaveDashboardAs("QqA".into()))
    );
    assert!(workspace.name_entry.is_none() && workspace.pending.is_some());
    workspace.pending = None;
    // Rename starts from the selected name; an empty name is refused.
    workspace.apply(MetricsWorkspaceAction::Select(0)).unwrap();
    workspace
        .apply(MetricsWorkspaceAction::RenameDashboard)
        .unwrap();
    assert_eq!(workspace.name_entry.as_ref().unwrap().text, "Team");
    for _ in 0..4 {
        workspace
            .apply(MetricsWorkspaceAction::NameBackspace)
            .unwrap();
    }
    assert!(workspace.apply(MetricsWorkspaceAction::NameSubmit).is_err());
    for character in "Review".chars() {
        workspace
            .apply(MetricsWorkspaceAction::NameChar(character))
            .unwrap();
    }
    assert_eq!(
        workspace.apply(MetricsWorkspaceAction::NameSubmit).unwrap(),
        Some(MetricsAction::RenameDashboard {
            from: "Team".into(),
            to: "Review".into()
        })
    );
    workspace.pending = None;
    // Esc discards a name without a request.
    workspace
        .apply(MetricsWorkspaceAction::NewDashboard)
        .unwrap();
    let discard = metrics_ui::name_entry_action(key(KeyCode::Esc), &workspace).unwrap();
    assert_eq!(workspace.apply(discard).unwrap(), None);
    assert!(workspace.name_entry.is_none());
    // Delete arms first, a second x confirms; moving the selection disarms.
    workspace.apply(MetricsWorkspaceAction::Select(1)).unwrap();
    assert_eq!(
        workspace
            .apply(MetricsWorkspaceAction::DeleteDashboard)
            .unwrap(),
        None
    );
    assert_eq!(workspace.delete_armed.as_deref(), Some("Ops"));
    assert!(metrics_ui::footer_keys(&workspace)
        .iter()
        .any(|control| control.label == "Confirm delete" && control.selected));
    workspace.apply(MetricsWorkspaceAction::Select(0)).unwrap();
    assert!(workspace.delete_armed.is_none());
    workspace.apply(MetricsWorkspaceAction::Select(1)).unwrap();
    workspace
        .apply(MetricsWorkspaceAction::DeleteDashboard)
        .unwrap();
    assert_eq!(
        workspace
            .apply(MetricsWorkspaceAction::DeleteDashboard)
            .unwrap(),
        Some(MetricsAction::DeleteDashboard("Ops".into()))
    );
    workspace.pending = None;
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert_eq!(workspace.view, MetricsView::Cards);
}
