//! Staged Dashboard code/widget contracts, not native PTY evidence.
#[path = "dashboard_layout_v1_tests.rs"]
mod dashboard_layout_v1;
use super::*;
use crate::{
    metrics::{
        dashboard, EventCollection, MetricActor, PrCohortKind, PrEvidence, PrState, PrTarget,
    },
    metrics_ui,
    providers::gh::{Actor, HostIdentity, MetricsActor, MetricsPrState, MetricsRepository},
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};

fn prepared() -> MetricsWorkspace {
    let identity = HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "fixture".into(),
    };
    let scope =
        crate::metrics::MetricsScope::new(&identity, &["example/a".into(), "example/b".into()])
            .unwrap();
    let actor = Actor {
        login: "octocat".into(),
        actor_type: "User".into(),
        ..Actor::default()
    };
    let target = PrTarget {
        repository: "example/a".into(),
        number: 7,
        repository_id: Some(9),
        database_id: Some(107),
        node_id: Some("genuine-pr-node".into()),
        source_record_id: None,
    };
    let pr = PrEvidence {
        target: target.clone(),
        cohort_member: true,
        title: "Exact contributor".into(),
        author: MetricActor {
            account_id: Some(42),
            actor: actor.clone(),
        },
        state: PrState::Open,
        draft: Some(false),
        head_sha: Some("a".repeat(40)),
        created_at: Some(1_700_000_000),
        review_requested_now: Some(true),
        timeline: EventCollection::unavailable(),
        reviews: EventCollection::unavailable(),
    };
    let evidence = Arc::new(DashboardEvidence {
        scope,
        cohort: PrCohortKind::SelectedRepositoryInventory,
        prs: vec![pr],
        pr_coverage: EvidenceCoverage {
            state: CoverageState::Complete,
            complete_empty: false,
            fetched_at: Some(1_800_000_000),
        },
        checks: vec![],
        check_coverage: EvidenceCoverage::unavailable("Checks were not requested"),
        runs: vec![],
        run_coverage: EvidenceCoverage::unavailable("Actions were not requested"),
    });
    let snapshot = Arc::new(
        dashboard(
            &evidence,
            1_800_000_000,
            UtcWindow::new(1_800_000_000 - 7 * 86_400, 1_800_000_000).unwrap(),
        )
        .unwrap(),
    );
    let observation = MetricsPrObservation {
        repository: MetricsRepository {
            host: "github.invalid".into(),
            id: 9,
            node_id: "genuine-repo-node".into(),
            full_name: "example/a".into(),
        },
        id: 107,
        node_id: "genuine-pr-node".into(),
        number: 7,
        title: "Exact contributor".into(),
        author: MetricsActor {
            account_id: Some(42),
            actor,
        },
        state: MetricsPrState::Open,
        draft: Some(false),
        created_at: None,
        merged_at: None,
        closed_at: None,
        merged: Some(false),
        head_sha: Some("a".repeat(40)),
        requested_users: Some(vec![]),
        requested_teams: Some(vec![]),
        observed_updated_at: None,
        reasons: vec![],
    };
    let repositories = vec![
        observation.repository.clone(),
        MetricsRepository {
            host: "github.invalid".into(),
            id: 10,
            node_id: "genuine-repo-b".into(),
            full_name: "example/b".into(),
        },
    ];
    MetricsWorkspace::prepare(
        evidence,
        snapshot,
        vec!["example/a".into(), "example/b".into()],
        Arc::from(vec![observation]),
        Arc::from(repositories),
    )
    .unwrap()
}
fn enter(workspace: &mut MetricsWorkspace) -> Option<MetricsAction> {
    let action = workspace.selected_action().unwrap();
    workspace.apply(action).unwrap()
}

#[test]
fn metrics_dashboard_cards_contributors_exact_open_and_back_preserve_window_scope_position() {
    let mut workspace = prepared();
    workspace.selected = 1;
    workspace.top = 1;
    let original_window = workspace.snapshot.window;
    assert!(enter(&mut workspace).is_none());
    assert_eq!(workspace.view, MetricsView::Contributors(1));
    let request = enter(&mut workspace).unwrap();
    assert!(matches!(
        request,
        MetricsAction::Open(MetricTarget::PullRequest(PrTarget {
            database_id: Some(107),
            ..
        }))
    ));
    assert!(workspace.pending.is_none());
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert_eq!(
        (workspace.view, workspace.selected, workspace.top),
        (MetricsView::Cards, 1, 1)
    );
    assert_eq!(workspace.snapshot.window, original_window);
    assert_eq!(workspace.scope_label, "Repositories: ALL 2 declared");
}
#[test]
fn metrics_dashboard_window_request_hides_previous_values_and_open_until_worker_prepares() {
    let mut workspace = prepared();
    let action = workspace.window_action(30);
    let Some(MetricsAction::Window(window)) = workspace.apply(action).unwrap() else {
        panic!("Expected typed window worker request")
    };
    assert_eq!(window.end, 1_800_000_000);
    assert_eq!(window.start, 1_800_000_000 - 30 * 86_400);
    assert!(workspace.rows().is_empty());
    assert!(workspace.selected_action().is_none());
    assert!(workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Open(
            MetricTarget::PullRequest(workspace.evidence.prs[0].target.clone())
        )))
        .is_err());
}
#[test]
fn metrics_dashboard_repo_picker_never_broadens_scope_and_back_retains_contributors() {
    let mut workspace = prepared();
    enter(&mut workspace);
    workspace
        .apply(MetricsWorkspaceAction::Repositories)
        .unwrap();
    assert_eq!(workspace.rows().len(), 3);
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert_eq!(workspace.view, MetricsView::Contributors(0));
    assert!(workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Repository(
            "other/secret".into()
        )))
        .is_err());
    let effect = workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Repository(
            "EXAMPLE/A".into(),
        )))
        .unwrap();
    assert_eq!(effect, Some(MetricsAction::Repository("EXAMPLE/A".into())));
    assert!(workspace.rows().is_empty());
}
#[test]
fn metrics_dashboard_unknown_censored_notes_actor_identity_and_formulas_are_prepared_not_guessed() {
    let mut workspace = prepared();
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Card(
            MetricKind::ReviewWait,
        )))
        .unwrap();
    assert!(workspace.cards[3].summary.contains("Unknown 1"));
    assert!(workspace.cards[3].summary.contains("Censored 0"));
    workspace.apply(MetricsWorkspaceAction::Inspect(0)).unwrap();
    assert!(workspace
        .rows()
        .iter()
        .any(|row| row.label.contains("Outstanding request timestamp")));
    let identity = workspace
        .rows()
        .iter()
        .find_map(|row| row.actor.as_ref())
        .unwrap();
    assert_eq!(identity.login.as_deref(), Some("octocat"));
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    assert_eq!(workspace.view, MetricsView::Contributors(3));
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Back))
        .unwrap();
    workspace.apply(MetricsWorkspaceAction::Inspect(3)).unwrap();
    assert!(workspace
        .rows()
        .iter()
        .any(|row| row.label.contains("Mean(as-of UTC")));
}
#[test]
fn metrics_dashboard_canonical_missing_replaced_parent_and_undeclared_scope_fail_closed() {
    let workspace = prepared();
    let mut observations = workspace.observations.to_vec();
    observations[0].id = 999;
    assert!(
        MetricsWorkspace::prepare(
            workspace.evidence.clone(),
            workspace.snapshot.clone(),
            workspace.declared_repositories.to_vec(),
            Arc::from(observations),
            workspace.repositories.clone(),
        )
        .is_err(),
        "A replaced genuine parent cannot publish old counts"
    );
    let mut snapshot = (*workspace.snapshot).clone();
    for card in &mut snapshot.cards {
        for contributor in &mut card.contributors {
            if let MetricTarget::PullRequest(pr) = &mut contributor.target {
                pr.repository_id = None;
                pr.database_id = None;
                pr.node_id = None;
            }
        }
    }
    let mut changed = MetricsWorkspace::prepare(
        workspace.evidence.clone(),
        Arc::new(snapshot),
        workspace.declared_repositories.to_vec(),
        workspace.observations.clone(),
        workspace.repositories.clone(),
    )
    .unwrap();
    enter(&mut changed);
    assert!(matches!(
        changed.selected_action(),
        Some(MetricsWorkspaceAction::Explain(_))
    ));
    assert!(changed
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Open(
            MetricTarget::PullRequest(PrTarget {
                repository_id: None,
                database_id: None,
                node_id: None,
                ..changed.evidence.prs[0].target.clone()
            })
        )))
        .is_err());
    for head in [None, Some("a".repeat(7))] {
        let mut observations = workspace.observations.to_vec();
        observations[0].head_sha = head;
        let mut unavailable = MetricsWorkspace::prepare(
            workspace.evidence.clone(),
            workspace.snapshot.clone(),
            workspace.declared_repositories.to_vec(),
            Arc::from(observations),
            workspace.repositories.clone(),
        )
        .unwrap();
        assert!(
            unavailable.card_rows[0]
                .label
                .contains("Loaded open PRs: 1"),
            "Missing head does not erase valid current open-PR count"
        );
        enter(&mut unavailable);
        assert!(matches!(
            unavailable.selected_action(),
            Some(MetricsWorkspaceAction::Explain(_))
        ));
        assert!(unavailable
            .apply(MetricsWorkspaceAction::Metric(MetricsAction::Open(
                MetricTarget::PullRequest(unavailable.evidence.prs[0].target.clone())
            )))
            .is_err());
    }
    assert!(MetricsWorkspace::prepare(
        workspace.evidence,
        workspace.snapshot,
        vec!["example/a".into()],
        workspace.observations,
        workspace.repositories
    )
    .is_err());
}
#[test]
fn metrics_dashboard_prepared_clones_share_evidence_snapshot_rows_and_details() {
    let workspace = prepared();
    let cloned = workspace.clone();
    assert!(Arc::ptr_eq(&workspace.snapshot, &cloned.snapshot));
    assert!(Arc::ptr_eq(&workspace.evidence, &cloned.evidence));
    assert!(Arc::ptr_eq(&workspace.cards, &cloned.cards));
    assert!(Arc::ptr_eq(&workspace.observations, &cloned.observations));
    assert!(Arc::ptr_eq(
        &workspace.cards[0].details,
        &cloned.cards[0].details
    ));
}
#[test]
fn metrics_dashboard_utc_labels_handle_epoch_leap_centuries_and_unavailable_extremes() {
    assert_eq!(utc_label(0), "1970-01-01 00:00:00 UTC");
    assert_eq!(
        utc_label(crate::metrics::utc_seconds("2000-02-29T23:59:59Z").unwrap()),
        "2000-02-29 23:59:59 UTC"
    );
    assert_eq!(
        utc_label(crate::metrics::utc_seconds("2100-03-01T00:00:00Z").unwrap()),
        "2100-03-01 00:00:00 UTC"
    );
    assert!(utc_label(u64::MAX).contains("calendar unavailable"));
}
#[test]
fn design_footer_v90_dashboard_controls_keyboard_mouse_and_resize_have_same_actions() {
    for (width, height) in [(140, 42), (120, 40), (80, 24)] {
        let mut workspace = prepared();
        let controls = metrics_ui::footer_keys(&workspace);
        let theme = crate::theme::Theme::new(
            crate::config::ThemeName::CatppuccinMocha,
            crate::theme::ColorMode::NoColor,
        );
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
        assert!(rendered.visible_rows >= 8);
        for (key, expected) in [
            ('7', workspace.window_action(7)),
            ('0', workspace.window_action(30)),
            ('R', MetricsWorkspaceAction::Repositories),
            ('r', MetricsWorkspaceAction::Metric(MetricsAction::Refresh)),
            ('i', MetricsWorkspaceAction::Inspect(workspace.selected)),
            ('b', MetricsWorkspaceAction::Metric(MetricsAction::Back)),
        ] {
            let expected = crate::app::Action::Metrics(expected);
            let keyboard = crate::footer::resolve_key(
                &controls,
                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
            )
            .unwrap();
            assert_eq!(keyboard, expected);
            // Repositories leaves the 80-column bar; every other key stays clickable.
            match targets.iter().find(|(_, action)| *action == keyboard) {
                Some((rect, _)) => {
                    assert!(rect.right() <= width && rect.bottom() <= height)
                }
                None => assert!(key == 'R' && width < 100, "{key} painted at {width}"),
            }
        }
        // Selection and movement stay hand-written; painted keys are not duplicated there.
        assert!(metrics_ui::keyboard_action(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            &workspace
        )
        .is_none());
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        for expected in [
            "Cohort: selected-repository",
            "UTC window [",
            "As-of",
            "Sources: PR Complete",
            "Checks Unavailable",
            "Actions Unavailable",
        ] {
            assert!(
                text.contains(expected),
                "Missing visible {expected} at {width}x{height}"
            );
        }
    }
}
#[test]
fn metrics_dashboard_keyboard_page_scroll_and_hover_wheel_only_move_current_panel() {
    let mut workspace = prepared();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut output = None;
    terminal
        .draw(|frame| {
            output = Some(metrics_ui::draw(
                frame,
                &mut workspace,
                Rect::new(0, 0, 80, 24),
                crate::theme::Theme::new(
                    crate::config::ThemeName::CatppuccinMocha,
                    crate::theme::ColorMode::NoColor,
                ),
                &Default::default(),
                true,
            ))
        })
        .unwrap();
    let output = output.unwrap();
    let action = metrics_ui::keyboard_action(
        KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
        &workspace,
    )
    .unwrap();
    workspace.apply(action).unwrap();
    assert_eq!(workspace.selected, 7);
    let wheel = |column, row| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };
    assert!(metrics_ui::mouse_action(wheel(0, 0), &workspace, &output).is_none());
    let action = metrics_ui::mouse_action(
        wheel(output.content.x, output.content.y),
        &workspace,
        &output,
    )
    .unwrap();
    workspace.apply(action).unwrap();
    assert_eq!(workspace.selected, 4);
}
#[test]
fn metrics_dashboard_pending_controls_explain_and_never_render_obsolete_counts_or_actor_rows() {
    let mut workspace = prepared();
    workspace
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Refresh))
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut output = None;
    terminal
        .draw(|frame| {
            output = Some(metrics_ui::draw(
                frame,
                &mut workspace,
                Rect::new(0, 0, 80, 24),
                crate::theme::Theme::new(
                    crate::config::ThemeName::CatppuccinMocha,
                    crate::theme::ColorMode::NoColor,
                ),
                &Default::default(),
                true,
            ))
        })
        .unwrap();
    // Pending hides every row; every footer control leaves the bar and its key explains.
    assert!(output.unwrap().hits.is_empty());
    let controls = metrics_ui::footer_keys(&workspace);
    assert!(crate::footer::layout(&controls, 140).is_empty());
    for key in ['7', '0', 'R', 'r', 'i', 'b', 'L', 'S', 'O', 'D'] {
        assert_eq!(
            crate::footer::resolve_key(
                &controls,
                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)
            ),
            Some(crate::app::Action::Explain(
                "Dashboard is preparing; previous values/Open are hidden".into()
            ))
        );
    }
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("prior values hidden"));
    // No card row (name or value) and no table header survives while preparing.
    assert!(!text.contains("Loaded open PRs") && !text.contains("VALUE"));
    assert!(!text.contains("Exact contributor"));
}

#[test]
fn metrics_dashboard_complete_empty_inventory_still_retains_exact_canonical_repository_headers() {
    let previous = prepared();
    let mut evidence = (*previous.evidence).clone();
    evidence.prs.clear();
    evidence.pr_coverage.complete_empty = true;
    let snapshot =
        Arc::new(dashboard(&evidence, previous.snapshot.as_of, previous.snapshot.window).unwrap());
    let empty = MetricsWorkspace::prepare(
        Arc::new(evidence.clone()),
        snapshot.clone(),
        previous.declared_repositories.to_vec(),
        Arc::from(Vec::<MetricsPrObservation>::new()),
        previous.repositories.clone(),
    )
    .unwrap();
    assert_eq!(empty.repositories.len(), 2);
    assert_eq!(empty.repositories[0].id, 9);
    assert_eq!(empty.repositories[1].id, 10);
    assert!(empty.card_rows[0].label.contains("Loaded open PRs: 0"));
    assert!(empty.observations.is_empty());
    assert!(MetricsWorkspace::prepare(
        Arc::new(evidence.clone()),
        snapshot.clone(),
        previous.declared_repositories.to_vec(),
        Arc::from(Vec::<MetricsPrObservation>::new()),
        Arc::from(Vec::<MetricsRepository>::new())
    )
    .is_err());
    let mut wrong = previous.repositories.to_vec();
    wrong[1].id = 0;
    assert!(MetricsWorkspace::prepare(
        Arc::new(evidence),
        snapshot,
        previous.declared_repositories.to_vec(),
        Arc::from(Vec::<MetricsPrObservation>::new()),
        Arc::from(wrong)
    )
    .is_err());
}

#[test]
fn metrics_dashboard_partial_missing_repository_headers_keep_scope_known_counts_and_disable_cached_window(
) {
    let previous = prepared();
    let mut evidence = (*previous.evidence).clone();
    evidence
        .pr_coverage
        .partial("example/b: authenticated repository access unavailable");
    let snapshot =
        Arc::new(dashboard(&evidence, previous.snapshot.as_of, previous.snapshot.window).unwrap());
    let mut partial = MetricsWorkspace::prepare(
        Arc::new(evidence),
        snapshot,
        previous.declared_repositories.to_vec(),
        previous.observations,
        Arc::from(vec![previous.repositories[0].clone()]),
    )
    .unwrap();
    assert_eq!(partial.scope_label, "Repositories: ALL 2 declared");
    assert_eq!(
        partial.missing_repositories.as_ref(),
        &["example/b".to_string()]
    );
    assert!(partial.card_rows[0].label.contains("Loaded open PRs: 1"));
    assert!(partial.source_coverage_label.contains("PR Partial"));
    assert!(partial.source_coverage_label.contains("Missing 1"));
    assert!(matches!(
        partial.window_action(7),
        MetricsWorkspaceAction::Explain(_)
    ));
    assert!(partial
        .apply(MetricsWorkspaceAction::Metric(MetricsAction::Window(
            partial.snapshot.window
        )))
        .is_err());
    assert_eq!(
        partial
            .apply(MetricsWorkspaceAction::Metric(MetricsAction::Refresh))
            .unwrap(),
        Some(MetricsAction::Refresh)
    );
}

#[test]
fn metrics_dashboard_unicode_wrap_is_incremental_and_caps_raw_grapheme_reason_and_prepared_bytes() {
    let text = "e\u{0301}".repeat(500);
    let rows = wrapped(&text, SemanticToken::Muted).unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row.label.as_str())
            .collect::<String>(),
        text
    );
    assert!(rows
        .iter()
        .all(|row| crate::editor::display_width(&row.label) <= WRAP_CELLS
            && row.label.len() <= MAX_ROW_BYTES));
    assert!(wrapped(
        &format!("e{}", "\u{0301}".repeat(MAX_ROW_BYTES)),
        SemanticToken::Muted
    )
    .is_err());
    assert!(wrapped(&"x".repeat(MAX_FIELD_BYTES + 1), SemanticToken::Muted).is_err());
    let workspace = prepared();
    let mut snapshot = (*workspace.snapshot).clone();
    snapshot.cards[0].coverage.state =
        CoverageState::Partial(vec!["x".repeat(MAX_FIELD_BYTES + 1)]);
    assert!(
        MetricsWorkspace::prepare(
            workspace.evidence,
            Arc::new(snapshot),
            workspace.declared_repositories.to_vec(),
            workspace.observations,
            workspace.repositories
        )
        .is_err(),
        "Reject oversized reasons before join/wrap/allocation"
    );
    let mut rows_count = 0;
    let mut bytes = MAX_METADATA_BYTES;
    assert!(prepared_budget(
        &[row("one".into(), SemanticToken::Normal, None)],
        &mut rows_count,
        &mut bytes
    )
    .is_err());
}

#[test]
fn design_help_v97_dashboard_help_and_commands_cover_every_footer_key() {
    let app = crate::app::App {
        view: crate::app::View::Dashboard,
        metrics: Some(prepared()),
        ..Default::default()
    };
    assert!(crate::navigation_ui::assert_help_and_palette_cover_footer_v97(&app, "Dashboard") > 5);
}

fn render_v91(
    workspace: &mut MetricsWorkspace,
    width: u16,
    height: u16,
) -> (ratatui::buffer::Buffer, metrics_ui::MetricsRender) {
    let theme = crate::theme::Theme::new(
        crate::config::ThemeName::CatppuccinMocha,
        crate::theme::ColorMode::TrueColor,
    );
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut output = None;
    terminal
        .draw(|frame| {
            output = Some(metrics_ui::draw(
                frame,
                workspace,
                Rect::new(0, 0, width, height),
                theme,
                &Default::default(),
                true,
            ))
        })
        .unwrap();
    (terminal.backend().buffer().clone(), output.unwrap())
}
fn lines_v91(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

#[test]
fn design_table_v91_metrics_cards_and_contributors_are_tables_with_the_selection_bar() {
    use ratatui::style::Modifier;
    let selection = crate::theme::rgb(
        crate::theme::Palette::for_theme(crate::config::ThemeName::CatppuccinMocha)
            .unwrap()
            .selection,
    );
    for (width, height) in [(80, 24), (140, 42)] {
        let mut workspace = prepared();
        let (buffer, rendered) = render_v91(&mut workspace, width, height);
        let lines = lines_v91(&buffer);
        let header = lines
            .iter()
            .position(|line| line.contains("METRIC "))
            .expect("Cards header");
        // Column titles start after the bar and glyph slot; the value has its own column.
        assert_eq!(lines[header].find("METRIC "), Some(3), "{width}");
        assert!(lines[header].contains("VALUE"));
        assert_eq!(usize::from(rendered.content.y), header);
        let first = &lines[header + 1];
        assert!(first.starts_with("▌"), "{first:?}");
        assert!(first.contains("Loaded open PRs  "), "{first:?}");
        assert!(
            !first.contains("Loaded open PRs:"),
            "the label is not the list text"
        );
        // The whole selected row is on the Selection background, never reversed.
        let y = header as u16 + 1;
        for x in 0..width {
            assert_eq!(buffer[(x, y)].bg, selection, "x={x}");
            assert!(!buffer[(x, y)].modifier.contains(Modifier::REVERSED));
        }
        assert_ne!(buffer[(5, y + 1)].bg, selection);
        // Every card row is one click target with its card action; the header is not.
        assert_eq!(rendered.hits.len(), workspace.rows().len());
        assert_eq!(rendered.hits[0].rect, Rect::new(0, y, width, 1));
        assert_eq!(
            Some(rendered.hits[0].action.clone()),
            workspace.rows()[0].action
        );
        assert!(lines.iter().all(|line| !line.contains("> ")));

        workspace.selected = 1;
        assert!(enter(&mut workspace).is_none());
        let (buffer, rendered) = render_v91(&mut workspace, width, height);
        let lines = lines_v91(&buffer);
        let header = lines
            .iter()
            .position(|line| line.contains("ITEM") && line.contains("TITLE"))
            .expect("Contributors header");
        let row = &lines[header + 1];
        assert!(row.starts_with("▌"), "{row:?}");
        // ITEM, TITLE, then the actor chip and login without `@`, then the measurement.
        let item = row.find("example/a#7").unwrap();
        let title = row.find("Exact contributor").unwrap();
        let actor = row.find("OC octocat").unwrap();
        assert!(item < title && title < actor, "{row:?}");
        assert!(!row.contains("@octocat") && !row.contains(" • "));
        assert!(row.trim_end().ends_with("current"), "{row:?}");
        assert_eq!(
            Some(rendered.hits[0].action.clone()),
            workspace.rows()[0].action
        );
    }
}
