//! Real editor/TestBackend contracts complement real-binary native journeys.
use super::*;
use ratatui::{backend::TestBackend, Terminal};

fn key(
    dialog: &mut CatalogueDiscoveryLimitsDialogV1,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Option<DiscoveryLimitsActionV1> {
    let action = dialog.key(KeyEvent::new(code, modifiers));
    if let Some(action) = &action {
        dialog.apply_local(action);
    }
    action
}
fn render(
    dialog: &mut CatalogueDiscoveryLimitsDialogV1,
    width: u16,
    height: u16,
) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            dialog.draw(
                frame,
                Rect::new(0, 0, width, height),
                Theme::from_settings(0, true),
            )
        })
        .unwrap();
    terminal
}
fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn mouse(rect: Rect, kind: MouseEventKind) -> MouseEvent {
    MouseEvent {
        kind,
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }
}
fn hit(dialog: &CatalogueDiscoveryLimitsDialogV1, action: &DiscoveryLimitsActionV1) -> Rect {
    dialog
        .hits
        .iter()
        .find(|(_, actual)| actual == action)
        .unwrap_or_else(|| panic!("Rendered action missing: {action:?}"))
        .0
}

#[test]
fn repo_catalog_read_limits_dialog_all_numeric_fields_require_explicit_apply_or_reread() {
    let mut dialog = CatalogueDiscoveryLimitsDialogV1::new(
        CatalogueDiscoveryBudgetV1::default(),
        DiscoveryLimitsPurposeV1::ReRead,
    );
    for (index, wanted) in ["21", "2100", "12", "24", "1"].into_iter().enumerate() {
        assert_eq!(dialog.focus, index);
        assert!(key(&mut dialog, KeyCode::Char('a'), KeyModifiers::CONTROL).is_none());
        dialog.paste(wanted);
        assert_eq!(dialog.fields[index].text, wanted);
        assert!(
            key(&mut dialog, KeyCode::Enter, KeyModifiers::NONE).is_none(),
            "Enter in an editor is not transport admission"
        );
    }
    assert_eq!(dialog.focus, 0);
    let budget = dialog.budget().unwrap().limits();
    assert_eq!(budget.max_pages, 21);
    assert_eq!(budget.max_rows, 2100);
    assert_eq!(budget.max_duration.as_secs(), 1);
    assert_eq!(
        key(&mut dialog, KeyCode::F(5), KeyModifiers::NONE),
        Some(DiscoveryLimitsActionV1::ReRead)
    );
    assert_eq!(
        key(&mut dialog, KeyCode::Esc, KeyModifiers::NONE),
        Some(DiscoveryLimitsActionV1::Back)
    );
}

#[test]
fn repo_catalog_read_limits_dialog_every_preset_apply_back_and_reread_has_identical_key_hit_action()
{
    for (width, height) in [(120, 40), (80, 24), (40, 18)] {
        let mut dialog = CatalogueDiscoveryLimitsDialogV1::new(
            CatalogueDiscoveryBudgetV1::default(),
            DiscoveryLimitsPurposeV1::ReRead,
        );
        for index in 0..3 {
            dialog.focus = 5 + index;
            dialog.manual_scroll = false;
            render(&mut dialog, width, height);
            let action = DiscoveryLimitsActionV1::Preset(index);
            let click = dialog
                .mouse(mouse(
                    hit(&dialog, &action),
                    MouseEventKind::Down(MouseButton::Left),
                ))
                .unwrap();
            assert_eq!(click, action);
            assert_eq!(
                key(&mut dialog, KeyCode::Enter, KeyModifiers::NONE),
                Some(action.clone())
            );
            assert_eq!(
                dialog.budget().unwrap(),
                CatalogueDiscoveryBudgetV1::preset(index).unwrap()
            );
        }
        for (focus, action) in [
            (8, DiscoveryLimitsActionV1::Apply),
            (9, DiscoveryLimitsActionV1::ReRead),
            (10, DiscoveryLimitsActionV1::Back),
        ] {
            dialog.focus = focus;
            let terminal = render(&mut dialog, width, height);
            assert!(screen(&terminal).contains(DISCOVERY_LIMITS_TITLE));
            assert_eq!(
                dialog.mouse(mouse(
                    hit(&dialog, &action),
                    MouseEventKind::Down(MouseButton::Left)
                )),
                Some(action.clone())
            );
            assert_eq!(
                dialog.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                Some(action)
            );
        }
    }
}

#[test]
fn repo_catalog_read_limits_dialog_preinsert_caps_controls_unicode_and_invalid_bounds_never_change_other_values(
) {
    let mut dialog = CatalogueDiscoveryLimitsDialogV1::new(
        CatalogueDiscoveryBudgetV1::default(),
        DiscoveryLimitsPurposeV1::ApplyOnly,
    );
    let before = dialog.fields.each_ref().map(|field| field.text.clone());
    key(&mut dialog, KeyCode::Char('a'), KeyModifiers::CONTROL);
    for text in [
        "1000000",
        "\u{1b}[31m",
        "１２",
        "20\n",
        "-1",
        " PRIVATE_UID42_CANARY ",
    ] {
        dialog.paste(text);
        assert_eq!(
            dialog.fields.each_ref().map(|field| field.text.clone()),
            before
        );
        assert!(dialog.error.is_some());
    }
    dialog.paste("1001");
    assert!(dialog.budget().is_err());
    let terminal = render(&mut dialog, 80, 24);
    assert!(screen(&terminal).contains("Invalid limits • no request sent"));
    assert!(!screen(&terminal).contains("PRIVATE_UID42_CANARY"));
    assert_eq!(dialog.fields[1].text, "2000");
    assert_eq!(
        dialog.key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
        None,
        "Limits-only dialogue cannot expose re-read action"
    );
}

#[test]
fn repo_catalog_read_limits_dialog_numeric_mouse_caret_selection_and_keyboard_match_actual_bytes() {
    let mut dialog = CatalogueDiscoveryLimitsDialogV1::new(
        CatalogueDiscoveryBudgetV1::preset(2).unwrap(),
        DiscoveryLimitsPurposeV1::ApplyOnly,
    );
    dialog.focus = 1;
    render(&mut dialog, 80, 24);
    let area = dialog.editors[1];
    let action = dialog
        .mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 3,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        })
        .unwrap();
    dialog.apply_local(&action);
    assert_eq!(dialog.fields[1].cursor, 3);
    assert!(dialog
        .key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
        .is_none());
    assert_eq!(dialog.fields[1].cursor, 2);
    dialog.key(KeyEvent::new(KeyCode::End, KeyModifiers::SHIFT));
    dialog.paste("00");
    assert_eq!(dialog.fields[1].text, "1000");
    assert_eq!(dialog.fields[1].cursor, 4);
    assert_eq!(dialog.budget().unwrap().limits().max_rows, 1000);
}

#[test]
fn repo_catalog_read_limits_dialog_hover_wheel_pages_scroll_only_own_body_and_keep_focus() {
    let mut dialog = CatalogueDiscoveryLimitsDialogV1::new(
        CatalogueDiscoveryBudgetV1::default(),
        DiscoveryLimitsPurposeV1::ReRead,
    );
    render(&mut dialog, 40, 12);
    let before = dialog.fields.each_ref().map(|field| field.text.clone());
    let action = dialog
        .mouse(mouse(dialog.body, MouseEventKind::ScrollDown))
        .unwrap();
    assert_eq!(action, DiscoveryLimitsActionV1::Scroll(3));
    dialog.apply_local(&action);
    render(&mut dialog, 40, 12);
    assert!(dialog.top > 0);
    assert_eq!(dialog.focus, 0);
    let amount = dialog.view_rows as i32;
    assert_eq!(
        key(&mut dialog, KeyCode::PageDown, KeyModifiers::NONE),
        Some(DiscoveryLimitsActionV1::Scroll(amount))
    );
    assert_eq!(
        dialog.fields.each_ref().map(|field| field.text.clone()),
        before
    );
    assert_eq!(
        dialog.mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 39,
            row: 11,
            modifiers: KeyModifiers::NONE
        }),
        None
    );
}

#[test]
fn repo_catalog_read_limits_dialog_clear_resize_drag_and_release_cannot_reuse_a_hidden_action() {
    let mut dialog = CatalogueDiscoveryLimitsDialogV1::new(
        CatalogueDiscoveryBudgetV1::default(),
        DiscoveryLimitsPurposeV1::ReRead,
    );
    render(&mut dialog, 120, 40);
    let reread = hit(&dialog, &DiscoveryLimitsActionV1::ReRead);
    assert_eq!(
        dialog.mouse(mouse(reread, MouseEventKind::Drag(MouseButton::Left))),
        None
    );
    dialog.clear_layout();
    assert_eq!(
        dialog.mouse(mouse(reread, MouseEventKind::Down(MouseButton::Left))),
        None
    );
    render(&mut dialog, 80, 24);
    assert_eq!(
        dialog.mouse(mouse(reread, MouseEventKind::Down(MouseButton::Left))),
        None
    );
    let mut release = KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    assert_eq!(dialog.key(release), None);
    assert!(dialog
        .key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .is_none());
    assert_eq!(dialog.focus, 1);
}
