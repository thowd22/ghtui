//! Source-only pure widget contracts. They do not replace Root's task-specific
//! real-binary keyboard/mouse journeys, live guards or navigation integration.
use super::*;
use crate::{app::View, config::ThemeName, theme::ColorMode};
use ratatui::{
    backend::TestBackend,
    style::{Color, Modifier},
    Terminal,
};

fn entry(id: &str, label: &str, keywords: &[&str], action: Action) -> PaletteEntry {
    PaletteEntry {
        id: id.into(),
        label: label.into(),
        keywords: keywords.iter().map(|keyword| (*keyword).into()).collect(),
        action,
    }
}
fn global() -> Vec<PaletteEntry> {
    let mut entries = View::ALL
        .into_iter()
        .map(|view| {
            entry(
                &format!("open.{}", view.label().to_lowercase()),
                &format!("Open {}", view.label()),
                &["navigation"],
                Action::View(view),
            )
        })
        .collect::<Vec<_>>();
    // This pure catalogue has no inspector implementation/authority. Root will
    // supply its actual existing app action at native registration time.
    entries.push(entry(
        "inspect.capabilities",
        "Inspect capabilities",
        &["inventory", "host requirements"],
        Action::Explain("Read-only inventory fixture".into()),
    ));
    entries
}
fn theme() -> Theme {
    Theme::new(ThemeName::Dark, ColorMode::TrueColor)
}
fn render(
    palette: &mut CommandPalette,
    width: u16,
    height: u16,
    theme: Theme,
) -> (Terminal<TestBackend>, Vec<PaletteHit>, String) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = vec![];
    terminal
        .draw(|frame| {
            let area = frame.area();
            hits = palette.draw(frame, area, theme);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let text = (0..height)
        .map(|row| {
            (0..width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    (terminal, hits, text)
}
fn key(palette: &mut CommandPalette, code: KeyCode, modifiers: KeyModifiers) -> PaletteOutcome {
    palette.dispatch(
        palette
            .key(code, modifiers)
            .expect("expected widget key action"),
    )
}
fn hit(hits: &[PaletteHit], action: PaletteAction) -> Rect {
    hits.iter()
        .find(|hit| hit.action == action)
        .expect("expected genuinely rendered action")
        .rect
}
fn click(palette: &mut CommandPalette, rect: Rect) -> PaletteOutcome {
    let action = palette
        .mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y)
        .expect("actual current layout click action");
    palette.dispatch(action)
}
fn choose(outcome: PaletteOutcome) -> (usize, Action) {
    match outcome {
        PaletteOutcome::Chosen {
            original_index,
            action,
        } => (original_index, action),
        other => panic!("Expected explicit original action, got {other:?}"),
    }
}

#[test]
fn original_tree_viewport_palette_actual34_cells_reflows_same_catalogue_and_reuses_exact_width_arcs(
) {
    let reason = format!(
        "{} FINAL reason remains exact",
        "wide界e\u{301} ".repeat(22)
    );
    let mut palette = CommandPalette::new(vec![entry(
        "exact.original",
        "Exact original command",
        &[],
        Action::Explain(reason),
    )])
    .unwrap();
    let ids = palette
        .entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();
    let original = palette.entries[0].detail.concat();
    palette.prepare_width_v2(34).unwrap();
    let narrow = palette.detail_current_v2.as_ref().unwrap().2.clone();
    assert_eq!(narrow.concat(), original);
    assert!(narrow
        .iter()
        .all(|row| crate::editor::display_width(row) <= 34));
    let (_, hits, text) = render(&mut palette, 36, 20, theme());
    for action in [
        PaletteAction::Page(-1),
        PaletteAction::Page(1),
        PaletteAction::DetailScroll(-1),
        PaletteAction::DetailScroll(1),
        PaletteAction::Choose,
        PaletteAction::Close,
    ] {
        assert!(hits
            .iter()
            .any(|hit| hit.action == action && !hit.rect.is_empty()));
    }
    assert!(text.contains("[Enter Choose]") && text.contains("[Esc Close]"));
    let before_draw = palette.detail_current_v2.as_ref().unwrap().2.clone();
    render(&mut palette, 36, 20, theme());
    assert!(std::sync::Arc::ptr_eq(
        &before_draw,
        &palette.detail_current_v2.as_ref().unwrap().2
    ));
    palette.prepare_width_v2(74).unwrap();
    assert_eq!(palette.detail().concat(), original);
    palette.prepare_width_v2(34).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &narrow,
        &palette.detail_current_v2.as_ref().unwrap().2
    ));
    assert_eq!(
        palette
            .entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(palette.selected_index(), Some(0));
    let (_, hits, _) = render(&mut palette, 36, 20, theme());
    let (index, action) = choose(click(&mut palette, hit(&hits, PaletteAction::Choose)));
    assert_eq!(index, 0);
    assert!(
        matches!(action,Action::Explain(value) if value.contains("FINAL reason remains exact"))
    );
}

#[test]
fn original_tree_viewport_palette_resize_never_builds_in_draw_and_refuses_bad_width_without_consuming_action(
) {
    let mut palette =
        CommandPalette::new(vec![entry("actual", "Actual action", &[], Action::Help)]).unwrap();
    let (_, hits, text) = render(&mut palette, 36, 20, theme());
    assert!(text.contains("Detail unavailable"));
    assert!(palette.detail_current_v2.is_none());
    assert!(palette.prepare_width_v2(19).is_err());
    assert!(palette.entries[0].action.is_some());
    assert!(hits.iter().any(|hit| hit.action == PaletteAction::Close));
    palette.prepare_width_v2(34).unwrap();
    assert_eq!(
        choose(key(&mut palette, KeyCode::Enter, KeyModifiers::NONE)),
        (0, Action::Help)
    );
}

#[test]
fn navigation_palette_source_ids_and_raw_budgets_are_preflighted_before_preparation() {
    for id in ["", "bad id", "unsafe\u{202e}"] {
        assert!(CommandPalette::new(vec![entry(id, "Open safely", &[], Action::Help)]).is_err());
    }
    assert!(CommandPalette::new(vec![
        entry("same", "First", &[], Action::Help),
        entry("same", "Second", &[], Action::Settings)
    ])
    .is_err());
    assert!(CommandPalette::new(vec![entry("blank", "  ", &[], Action::Help)]).is_err());
    for label in [
        "bad\nlabel".into(),
        "\x1b[31mRed".into(),
        "界".repeat(MAX_FIELD_BYTES),
    ] {
        assert!(CommandPalette::new(vec![entry("unsafe", &label, &[], Action::Help)]).is_err());
    }
    assert!(CommandPalette::new(vec![entry(
        "keywords",
        "Open",
        &["word"; MAX_KEYWORDS + 1],
        Action::Help
    )])
    .is_err());
    assert!(CommandPalette::new(vec![entry(
        "keyword-bytes",
        "Open",
        &[&"a".repeat(MAX_KEYWORD_BYTES + 1)],
        Action::Help,
    )])
    .is_err());
    assert!(CommandPalette::new(vec![entry(
        "explain",
        "Explain",
        &[],
        Action::Explain("a".repeat(MAX_FIELD_BYTES + 1))
    )])
    .is_err());
    assert!(CommandPalette::new(
        (0..MAX_ENTRIES + 1)
            .map(|index| entry(&format!("id{index}"), "Open", &[], Action::Help))
            .collect()
    )
    .is_err());
    assert!(CommandPalette::new(
        (0..200)
            .map(|index| entry(
                &format!("id{index}"),
                &"a".repeat(MAX_FIELD_BYTES),
                &[],
                Action::Help
            ))
            .collect()
    )
    .is_err());
}

#[test]
fn navigation_palette_empty_query_retains_first_six_original_global_indices() {
    let mut palette = CommandPalette::new(global()).unwrap();
    assert_eq!(palette.visible_indices(), &[0, 1, 2, 3, 4, 5]);
    assert_eq!(palette.selected_id(), Some("open.inbox"));
    for _ in 0..5 {
        key(&mut palette, KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(palette.selected_id(), Some("inspect.capabilities"));
    assert_eq!(
        choose(key(&mut palette, KeyCode::Enter, KeyModifiers::NONE)),
        (5, Action::Explain("Read-only inventory fixture".into()))
    );
    let mut whitespace = CommandPalette::new(global()).unwrap();
    whitespace.dispatch(PaletteAction::Paste("  ".into()));
    assert_eq!(whitespace.visible_indices(), &[0, 1, 2, 3, 4, 5]);
}

#[test]
fn navigation_palette_fuzzy_and_exact_keywords_resolve_genuine_original_indices() {
    let mut palette = CommandPalette::new(vec![
        entry(
            "read",
            "Open repository",
            &["browse"],
            Action::View(View::Repository),
        ),
        entry(
            "work",
            "Open worktree changes",
            &["local files"],
            Action::RepositoryMode(crate::workspace::RepositoryMode::Working),
        ),
        entry("stage", "Inspect settings", &["unstage"], Action::Settings),
    ])
    .unwrap();
    palette.dispatch(PaletteAction::Insert("wrtree".into()));
    assert_eq!(palette.visible_indices(), &[1]);
    palette.dispatch(PaletteAction::ClearQuery);
    palette.dispatch(PaletteAction::Insert("unstage".into()));
    assert_eq!(palette.visible_indices(), &[2]);
    assert!(matches!(
        palette.dispatch(PaletteAction::IndexedRow(0)),
        PaletteOutcome::Rejected(_)
    ));
    assert!(matches!(
        palette.dispatch(PaletteAction::Activate(0)),
        PaletteOutcome::Rejected(_)
    ));
    assert_eq!(
        choose(palette.dispatch(PaletteAction::Choose)),
        (2, Action::Settings)
    );
}

#[test]
fn navigation_palette_score_ties_are_stable_without_reordering_empty_catalogue() {
    let mut palette = CommandPalette::new(vec![
        entry("id-b", "Open list", &["exact alias"], Action::Settings),
        entry("id-a", "Open list", &["exact alias"], Action::Help),
    ])
    .unwrap();
    palette.dispatch(PaletteAction::Insert("exact alias".into()));
    assert_eq!(palette.visible_indices(), &[0, 1]);
    palette.dispatch(PaletteAction::ClearQuery);
    assert_eq!(palette.visible_indices(), &[0, 1]);
    assert_eq!(
        choose(palette.dispatch(PaletteAction::Choose)),
        (0, Action::Settings)
    );
}

#[test]
fn navigation_palette_result_click_and_enter_emit_same_original_action_once() {
    let mut keyboard = CommandPalette::new(global()).unwrap();
    let mut mouse = CommandPalette::new(global()).unwrap();
    for palette in [&mut keyboard, &mut mouse] {
        palette.dispatch(PaletteAction::Insert("dashboard".into()));
    }
    let (_, hits, _) = render(&mut mouse, 120, 40, theme());
    let rect = hit(&hits, PaletteAction::Activate(4));
    assert_eq!(
        mouse.mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y),
        Some(PaletteAction::Activate(4))
    );
    assert_eq!(
        choose(click(&mut mouse, rect)),
        choose(key(&mut keyboard, KeyCode::Enter, KeyModifiers::NONE))
    );
    for palette in [&mut keyboard, &mut mouse] {
        assert_eq!(
            palette.dispatch(PaletteAction::Choose),
            PaletteOutcome::Ignored
        );
        assert_eq!(
            palette.dispatch(PaletteAction::Activate(0)),
            PaletteOutcome::Ignored
        );
        assert!(palette.visible_indices().is_empty());
        assert!(palette.query().is_empty());
        assert_eq!(
            palette.mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y),
            None
        );
    }
}

#[test]
fn navigation_palette_choose_button_and_keyboard_enter_share_one_shot_activation() {
    let mut keyboard = CommandPalette::new(global()).unwrap();
    let mut mouse = CommandPalette::new(global()).unwrap();
    for palette in [&mut keyboard, &mut mouse] {
        assert_eq!(
            palette.dispatch(PaletteAction::IndexedRow(3)),
            PaletteOutcome::Changed
        );
    }
    let (_, hits, _) = render(&mut mouse, 80, 24, theme());
    assert_eq!(
        choose(click(&mut mouse, hit(&hits, PaletteAction::Choose))),
        (3, Action::View(View::Actions))
    );
    assert_eq!(
        choose(key(&mut keyboard, KeyCode::Enter, KeyModifiers::NONE)),
        (3, Action::View(View::Actions))
    );
}

#[test]
fn navigation_palette_disabled_explanation_is_searchable_clickable_and_readable() {
    let reason = "Unavailable: administrative permission was not observed; no write will be sent.";
    let mut palette = CommandPalette::new(vec![
        entry("read", "Open view", &[], Action::Help),
        entry(
            "denied",
            "Edit runner labels",
            &["settings"],
            Action::Explain(reason.into()),
        ),
    ])
    .unwrap();
    palette.dispatch(PaletteAction::Paste("administrative".into()));
    assert_eq!(palette.visible_indices(), &[1]);
    let (_, hits, text) = render(&mut palette, 120, 40, theme());
    assert!(text.contains("[why unavailable]"));
    assert!(text.contains("administrative permission was not observed"));
    assert!(text.contains("Catalogue observation is not current mutation authority"));
    assert_eq!(
        choose(click(&mut palette, hit(&hits, PaletteAction::Activate(1)))),
        (1, Action::Explain(reason.into()))
    );
}

#[test]
fn navigation_palette_empty_missing_no_match_never_selects_an_unrelated_action() {
    let mut empty = CommandPalette::new(vec![]).unwrap();
    assert!(empty.selected_index().is_none());
    let (_, hits, text) = render(&mut empty, 80, 24, theme());
    assert!(text.contains("No commands supplied"));
    assert!(!hits
        .iter()
        .any(|hit| matches!(hit.action, PaletteAction::Activate(_))));
    assert_eq!(
        empty.dispatch(PaletteAction::Choose),
        PaletteOutcome::Ignored
    );
    assert!(matches!(
        empty.dispatch(PaletteAction::Activate(usize::MAX)),
        PaletteOutcome::Rejected(_)
    ));
    let mut no_match = CommandPalette::new(global()).unwrap();
    no_match.dispatch(PaletteAction::Insert("zzzzzzzzzzzzzzzzzzzz".into()));
    assert!(no_match.visible_indices().is_empty());
    assert_eq!(
        no_match.dispatch(PaletteAction::Choose),
        PaletteOutcome::Ignored
    );
    assert_eq!(
        key(&mut no_match, KeyCode::Esc, KeyModifiers::NONE),
        PaletteOutcome::Closed
    );
}

#[test]
fn navigation_palette_unicode_caret_selection_and_paste_bounds_are_atomic() {
    let mut palette = CommandPalette::new(global()).unwrap();
    palette.dispatch(PaletteAction::Paste("é界e\u{301}".into()));
    render(&mut palette, 80, 24, theme());
    let area = palette.query_area;
    assert_eq!(
        palette.mouse(MouseEventKind::Down(MouseButton::Left), area.x + 1, area.y),
        Some(PaletteAction::QueryClick(1))
    );
    palette.dispatch(PaletteAction::QueryClick(1));
    palette.dispatch(PaletteAction::Insert("X".into()));
    assert_eq!(palette.query(), "éX界e\u{301}");
    key(&mut palette, KeyCode::End, KeyModifiers::NONE);
    key(&mut palette, KeyCode::Left, KeyModifiers::SHIFT);
    key(&mut palette, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(palette.query(), "éX界");
    palette.dispatch(PaletteAction::SelectAll);
    let (selected_terminal, _, _) = render(
        &mut palette,
        80,
        24,
        Theme::new(ThemeName::NoColor, ColorMode::NoColor),
    );
    assert!(
        selected_terminal.backend().buffer()[(palette.query_area.x, palette.query_area.y)]
            .modifier
            .contains(Modifier::REVERSED)
    );
    let cursor = palette.query.cursor;
    let selection = palette.query.selection();
    for refused in [
        "x".repeat(513),
        "a".repeat(129),
        "unsafe\nquery".into(),
        "\u{202e}bad".into(),
    ] {
        assert!(matches!(
            palette.dispatch(PaletteAction::Paste(refused)),
            PaletteOutcome::Rejected(_)
        ));
        assert_eq!(palette.query(), "éX界");
        assert_eq!(palette.query.cursor, cursor);
        assert_eq!(palette.query.selection(), selection);
    }
    palette.dispatch(PaletteAction::Paste("🦀".repeat(128)));
    assert_eq!(palette.query().len(), MAX_QUERY_BYTES);
    render(&mut palette, 80, 24, theme());
    assert_eq!(palette.query.left_column % 2, 0);
    assert_eq!(palette.query.position().1, 256);
    assert!(matches!(
        palette.dispatch(PaletteAction::Insert("x".into())),
        PaletteOutcome::Rejected(_)
    ));
}

#[test]
fn navigation_palette_focus_tab_backtab_and_modifiers_are_keyboard_complete() {
    let mut palette = CommandPalette::new(global()).unwrap();
    assert_eq!(palette.focus, Focus::Query);
    key(&mut palette, KeyCode::Char('j'), KeyModifiers::NONE);
    assert_eq!(palette.query(), "j");
    key(&mut palette, KeyCode::Char('u'), KeyModifiers::CONTROL);
    key(&mut palette, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(palette.focus, Focus::List);
    key(&mut palette, KeyCode::Char('j'), KeyModifiers::NONE);
    assert_eq!(palette.selected_index(), Some(1));
    assert_eq!(palette.key(KeyCode::Char('q'), KeyModifiers::ALT), None);
    assert_eq!(palette.key(KeyCode::Enter, KeyModifiers::CONTROL), None);
    for focus in [Focus::Details, Focus::Choose, Focus::Close, Focus::Query] {
        key(&mut palette, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(palette.focus, focus);
    }
    key(&mut palette, KeyCode::BackTab, KeyModifiers::SHIFT);
    assert_eq!(palette.focus, Focus::Close);
    assert_eq!(
        key(&mut palette, KeyCode::Enter, KeyModifiers::NONE),
        PaletteOutcome::Closed
    );
}

#[test]
fn navigation_palette_hovered_wheel_focus_and_control_hits_share_typed_semantics() {
    let mut palette = CommandPalette::new(global()).unwrap();
    let (_, hits, _) = render(&mut palette, 80, 24, theme());
    let list = palette.list_area;
    let detail = palette.detail_area;
    assert_eq!(
        palette.mouse(MouseEventKind::ScrollDown, list.x, list.y),
        Some(PaletteAction::Move(1))
    );
    assert_eq!(
        palette.mouse(MouseEventKind::ScrollUp, detail.x, detail.y),
        Some(PaletteAction::DetailScroll(-1))
    );
    assert_eq!(palette.mouse(MouseEventKind::ScrollDown, 0, 0), None);
    assert_eq!(
        palette.mouse(MouseEventKind::Up(MouseButton::Left), list.x, list.y),
        None
    );
    assert_eq!(
        palette.mouse(MouseEventKind::Down(MouseButton::Left), detail.x, detail.y),
        Some(PaletteAction::FocusDetails)
    );
    assert_eq!(
        key(&mut palette, KeyCode::Tab, KeyModifiers::NONE),
        PaletteOutcome::Changed
    );
    assert_eq!(palette.focus, Focus::List);
    assert!(hits
        .iter()
        .any(|hit| hit.action == PaletteAction::FocusNext(false)));
    render(&mut palette, 80, 24, theme());
    palette.dispatch(PaletteAction::FocusDetails);
    assert_eq!(
        palette.key(KeyCode::Down, KeyModifiers::NONE),
        Some(PaletteAction::DetailScroll(1))
    );
    assert_eq!(
        palette.key(KeyCode::Up, KeyModifiers::NONE),
        Some(PaletteAction::DetailScroll(-1))
    );
}

#[test]
fn navigation_palette_page_edges_and_resize_retain_original_selection_and_safe_hits() {
    let entries = || {
        (0..20)
            .map(|index| {
                entry(
                    &format!("id{index}"),
                    &format!("Open item {index}"),
                    &[],
                    Action::Explain(format!("Requirement {index}")),
                )
            })
            .collect()
    };
    let mut keyboard = CommandPalette::new(entries()).unwrap();
    let mut mouse = CommandPalette::new(entries()).unwrap();
    render(&mut keyboard, 120, 40, theme());
    let (_, hits, _) = render(&mut mouse, 120, 40, theme());
    let expected_page = keyboard.list_size;
    key(&mut keyboard, KeyCode::PageDown, KeyModifiers::NONE);
    click(&mut mouse, hit(&hits, PaletteAction::Page(1)));
    assert_eq!(keyboard.selected_index(), Some(expected_page));
    assert_eq!(keyboard.selected_id(), mouse.selected_id());
    let old_query = mouse.query_area;
    mouse.clear_layout();
    assert_eq!(
        mouse.mouse(
            MouseEventKind::Down(MouseButton::Left),
            old_query.x,
            old_query.y
        ),
        None
    );
    let selected = mouse.selected_id().map(str::to_owned);
    let (_, hits, _) = render(&mut mouse, 80, 24, theme());
    assert_eq!(mouse.selected_id(), selected.as_deref());
    for hit in &hits {
        assert!(hit.rect.x + hit.rect.width <= 80 && hit.rect.y + hit.rect.height <= 24);
    }
    mouse.dispatch(PaletteAction::ListEdge(true));
    assert_eq!(mouse.selected_id(), Some("id19"));
    let (_, hits, _) = render(&mut mouse, 80, 24, theme());
    assert!(hits
        .iter()
        .any(|hit| hit.action == PaletteAction::Activate(19)));
    assert!(!hits
        .iter()
        .any(|hit| hit.action == PaletteAction::Activate(0)));
    key(&mut mouse, KeyCode::Home, KeyModifiers::NONE);
    assert_eq!(mouse.selected_id(), Some("id0"));
}

#[test]
fn navigation_palette_narrow_no_color_and_tiny_close_preserve_readable_action_parity() {
    let mut palette = CommandPalette::new(global()).unwrap();
    let no_color = Theme::new(ThemeName::NoColor, ColorMode::NoColor);
    let (terminal, hits, text) = render(&mut palette, 80, 24, no_color);
    assert!(text.contains("Open Inbox"));
    assert!(text.contains("[Enter Choose]") && text.contains("[Esc Close]"));
    let selected = hit(&hits, PaletteAction::Activate(0));
    assert!(terminal.backend().buffer()[(selected.x, selected.y)]
        .modifier
        .contains(Modifier::REVERSED));
    for cell in &terminal.backend().buffer().content {
        assert_eq!(cell.fg, Color::Reset);
        assert_eq!(cell.bg, Color::Reset);
    }
    let (_, tiny_hits, text) = render(&mut palette, 20, 5, no_color);
    assert!(text.contains("[Esc Close]"));
    assert_eq!(tiny_hits.len(), 1);
    assert_eq!(
        click(&mut palette, hit(&tiny_hits, PaletteAction::Close)),
        PaletteOutcome::Closed
    );
}

#[test]
fn navigation_palette_explanation_preparation_is_bounded_and_invalidation_drops_payloads() {
    assert!(CommandPalette::new(vec![entry(
        "rows",
        "Explain",
        &[],
        Action::Explain("\n".repeat(4096))
    )])
    .is_err());
    let reason = format!(
        "{}\nFINAL-REQUIREMENT",
        "Long original requirement ".repeat(140)
    );
    let mut palette = CommandPalette::new(vec![entry(
        "long",
        "Inspect requirements",
        &[],
        Action::Explain(reason.clone()),
    )])
    .unwrap();
    render(&mut palette, 80, 24, theme());
    palette.dispatch(PaletteAction::FocusDetails);
    key(&mut palette, KeyCode::End, KeyModifiers::NONE);
    let (_, _, text) = render(&mut palette, 80, 24, theme());
    assert!(text.contains("FINAL-REQUIREMENT"));
    assert_eq!(
        palette.entries[0].action.as_ref(),
        Some(&Action::Explain(reason))
    );
    let query = palette.query_area;
    palette.dispatch(PaletteAction::Insert("no-match".into()));
    assert_eq!(
        palette.mouse(MouseEventKind::Down(MouseButton::Left), query.x, query.y),
        None
    );
    palette.invalidate();
    assert_eq!(
        palette.dispatch(PaletteAction::Activate(0)),
        PaletteOutcome::Ignored
    );
    let (_, hits, text) = render(&mut palette, 120, 40, theme());
    assert!(hits.is_empty());
    assert!(!text.contains("original requirement"));
    assert!(palette.entries.is_empty() && palette.query().is_empty());
}

#[test]
fn v1_navigation_palette_lists_runnable_commands_before_unavailable_ones() {
    let entries = || {
        vec![
            entry(
                "blocked.a",
                "Merge pull request",
                &[],
                Action::Explain("Open a PR first".into()),
            ),
            entry("open.inbox", "Open Inbox", &[], Action::View(View::Inbox)),
            entry(
                "blocked.b",
                "Notify reviewers",
                &[],
                Action::Explain("Open a PR first".into()),
            ),
            entry(
                "open.notifications",
                "Open notifications inbox",
                &[],
                Action::Help,
            ),
            entry(
                "blocked.c",
                "Rerun",
                &[],
                Action::Explain("Explains notifications".into()),
            ),
        ]
    };
    let mut palette = CommandPalette::new(entries()).unwrap();
    assert_eq!(palette.visible_indices(), &[1, 3, 0, 2, 4]);
    assert_eq!(palette.selected_id(), Some("open.inbox"));
    // A label match ranks above an explanation-only match; runnable first.
    palette.dispatch(PaletteAction::Insert("notif".into()));
    assert_eq!(palette.visible_indices()[0], 3);
    assert_eq!(palette.visible_indices().last(), Some(&4));
    assert_eq!(
        choose(palette.dispatch(PaletteAction::Choose)),
        (3, Action::Help)
    );
    // Explanations stay searchable when nothing else matches.
    let mut only = CommandPalette::new(entries()).unwrap();
    only.dispatch(PaletteAction::Insert("explains".into()));
    assert_eq!(only.visible_indices(), &[4]);
}

#[test]
fn v1_navigation_palette_shows_key_hints_only_for_runnable_commands() {
    let mut palette = CommandPalette::new(vec![
        entry("open.inbox", "Open Inbox", &[], Action::View(View::Inbox)),
        entry(
            "blocked",
            "Merge pull request",
            &[],
            Action::Explain(
                "The app rechecks the current target and permissions before anything is written to GitHub."
                    .into(),
            ),
        ),
    ])
    .unwrap();
    palette.set_shortcut("open.inbox", "1");
    palette.set_shortcut("blocked", "M");
    palette.set_shortcut("missing", "x");
    palette.set_shortcut("open.inbox", "\u{1b}");
    assert_eq!(palette.shortcut("open.inbox"), Some("1"));
    assert_eq!(palette.shortcut("missing"), None);
    let (_, _, text) = render(&mut palette, 100, 30, theme());
    let row = text
        .lines()
        .find(|line| line.contains("> Open Inbox"))
        .unwrap();
    assert!(row.trim_end_matches(['│', ' ']).ends_with('1'), "{row}");
    let blocked = text
        .lines()
        .find(|line| line.contains("Merge pull request [why"))
        .unwrap();
    assert!(
        !blocked.trim_end_matches(['│', ' ']).ends_with('M'),
        "{blocked}"
    );
}
