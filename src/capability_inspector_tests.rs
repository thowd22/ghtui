use super::*;
use ratatui::{backend::TestBackend, Terminal};

fn identity(id: Option<u64>) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: id,
        gh_version: "gh version 2.101.0".into(),
    }
}
fn details(inspector: &CapabilityInspector) -> String {
    inspector
        .details()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn select(inspector: &mut CapabilityInspector, id: &str) {
    let index = inspector
        .rows
        .iter()
        .position(|row| row.id == id)
        .expect("genuine inventory row");
    assert!(!inspector.dispatch(InspectorAction::Row(index)));
    assert_eq!(inspector.selected_id(), Some(id));
}

#[test]
fn gh_provider_cli_inventory_exact_routes_keep_documented_flags_floors_and_authority_separate() {
    let mut inspector = CapabilityInspector::new(Some(&identity(Some(42))), None).unwrap();
    for (id, flag, floor) in [
        ("pr-details", "--slurp", "2.48.0"),
        ("pr-checks", "--attempt", "2.26.0"),
        ("actions-logs", "--attempt", "2.26.0"),
        ("pr-lifecycle", "--undo", "2.16.0"),
        ("pr-merge", "--match-head-commit", "2.13.0"),
        ("actions-secrets-variables", "--no-repos-selected", "2.75.0"),
    ] {
        select(&mut inspector, id);
        let text = details(&inspector);
        assert!(text.contains("CLI provider route only:"));
        assert!(text.contains(flag));
        assert!(text.contains(&format!("Documented local flag floor: gh {floor}")));
        assert!(text.contains(&format!("https://github.com/cli/cli/releases/tag/v{floor}")));
        assert!(text.contains("numeric release shape meets documented flag floor"));
        assert!(text.contains("not a fresh executable probe"));
        assert!(text.contains("current target permissions"));
        assert!(text.contains("not observed/granted"));
        assert!(!text.contains("permission granted"));
    }
    select(&mut inspector, "pr-metadata");
    assert!(details(&inspector).contains("no exact operation-flag floor reviewed"));
    assert!(!details(&inspector).contains("Documented local flag floor:"));
}

#[test]
fn gh_provider_cli_inventory_missing_old_custom_future_and_malformed_versions_stay_distinct() {
    for (raw, expected) in [
        ("gh version 2.12.0", "below documented flag floor"),
        (
            "gh version 2.13.0",
            "numeric release shape meets documented flag floor",
        ),
        (
            "gh version 2.101.0-custom",
            "custom/prerelease build; flag support unverified",
        ),
        ("gh version 3.0.0", "future major; flag support unverified"),
        (
            "gh version 2.",
            "no parsed last-observed version; flag support unverified",
        ),
    ] {
        let mut id = identity(Some(42));
        id.gh_version = raw.into();
        let mut inspector = CapabilityInspector::new(Some(&id), None).unwrap();
        select(&mut inspector, "pr-merge");
        let text = details(&inspector);
        assert!(text.contains(expected), "{raw}: {text}");
        assert!(text.contains("Minimum operation version: unverified"));
        assert!(text.contains("Never drop an exact head, attempt or scope flag"));
    }
    for id in [None, Some(0)] {
        let mut inspector = CapabilityInspector::new(Some(&identity(id)), None).unwrap();
        select(&mut inspector, "pr-merge");
        assert!(details(&inspector).contains("no parsed last-observed version"));
    }
}

#[test]
fn gh_provider_cli_inventory_flag_search_selects_only_exact_reviewed_route_categories() {
    let mut inspector = CapabilityInspector::new(None, None).unwrap();
    inspector.dispatch(InspectorAction::Insert("--attempt".into()));
    assert_eq!(
        inspector
            .visible
            .iter()
            .map(|&i| inspector.rows[i].id.as_str())
            .collect::<Vec<_>>(),
        vec!["pr-checks", "actions-logs"]
    );
    inspector.dispatch(InspectorAction::ClearFilter);
    inspector.dispatch(InspectorAction::Insert("--slurp".into()));
    assert_eq!(inspector.selected_id(), Some("pr-details"));
    assert_eq!(inspector.visible.len(), 1);
}
fn render(
    inspector: &mut CapabilityInspector,
    width: u16,
    height: u16,
    no_color: bool,
) -> (Vec<InspectorHit>, String) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = vec![];
    terminal
        .draw(|frame| {
            hits = inspector.draw(frame, frame.area(), Theme::from_settings(0, no_color));
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    if no_color {
        for cell in &buffer.content {
            assert_eq!(cell.fg, ratatui::style::Color::Reset);
            assert_eq!(cell.bg, ratatui::style::Color::Reset);
        }
    }
    let text = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    (hits, text)
}

#[test]
fn gh_provider_inspector_static_transport_presence_never_claims_native_or_live_capability() {
    let mut inspector = CapabilityInspector::new(Some(&identity(Some(42))), None).unwrap();
    select(&mut inspector, "pr-checks");
    let text = details(&inspector);
    assert!(text.contains("static inventory records transport present"));
    assert!(text.contains("not native parity or a live support claim"));
    assert!(text.contains("Availability: unverified"));
    assert!(text.contains("Minimum operation version: unverified"));
    assert!(text.contains("TASK-14"));
    assert!(text.contains("current host, permission and exact target"));
    assert!(inspector.header.contains("account #42"));
    assert!(inspector.header.contains("not permission authority"));
    assert!(inspector.header.contains("[?] @octocat [unknown]"), "The identity DTO does not supply actor kind/avatar; shared unknown fallback must not invent either");
    select(&mut inspector, "pr-comment-edit-delete-react");
    assert!(
        details(&inspector).contains("Inventory note (may precede newer task evidence)"),
        "Stale static false does not negate newer native task evidence"
    );
}

#[test]
fn gh_provider_inspector_missing_numeric_identity_and_denied_setup_keep_local_git_independent() {
    for id in [None, Some(0)] {
        let inspector =
            CapabilityInspector::new(Some(&identity(id)), Some("gh authentication unavailable"))
                .unwrap();
        assert!(inspector.header.contains("gh auth login"));
        assert!(!inspector.header.contains("@octocat"));
        assert!(details(&inspector).contains("gh authentication unavailable"));
        assert!(details(&inspector).contains("Local Git remains independent"));
        assert!(!details(&inspector).contains("permission granted"));
    }
}

#[test]
fn gh_provider_inspector_all_pr_and_actions_rows_keep_exact_tasks_and_permission_requirements() {
    let mut inspector = CapabilityInspector::new(None, None).unwrap();
    assert_eq!(inspector.rows.len(), 33);
    for (id, task, permission) in [
        ("pr-metadata", "TASK-18", "triage/write"),
        ("pr-inline", "TASK-19", "comment"),
        ("pr-pending-reviews", "TASK-20", "review/maintain"),
        ("pr-suggestions", "TASK-23", "push"),
        ("pr-queue", "TASK-24", "merge"),
        ("actions-artifacts", "TASK-56", "read/write"),
        ("actions-approval", "TASK-57", "write"),
        ("actions-policy", "TASK-60", "admin"),
        ("actions-runners", "TASK-60", "admin"),
    ] {
        select(&mut inspector, id);
        let text = details(&inspector);
        assert!(text.contains(task));
        assert!(text.contains(&format!(
            "Required permission class: {permission}; not observed/granted"
        )));
        assert!(text.contains("exact native workflow"));
        assert!(text.contains("concurrency requirement"));
    }
}

#[test]
fn gh_provider_inspector_filter_uses_genuine_rows_and_never_retargets_hidden_index() {
    let mut inspector = CapabilityInspector::new(None, None).unwrap();
    let hidden = inspector
        .rows
        .iter()
        .position(|row| row.id == "pr-merge")
        .unwrap();
    inspector.dispatch(InspectorAction::Insert("actions admin runners".into()));
    assert_eq!(inspector.selected_id(), Some("actions-runners"));
    inspector.dispatch(InspectorAction::Row(hidden));
    assert_eq!(inspector.selected_id(), Some("actions-runners"));
    inspector.dispatch(InspectorAction::Insert(" absent".into()));
    assert!(inspector.selected_id().is_none());
    assert!(inspector.details().is_empty());
    inspector.dispatch(InspectorAction::ClearFilter);
    assert_eq!(inspector.visible.len(), 33);
    assert_eq!(inspector.selected_id(), Some("personal-queues"));
}

#[test]
fn gh_provider_inspector_unicode_filter_caret_selection_and_oversized_paste_refusal() {
    let mut inspector = CapabilityInspector::new(None, None).unwrap();
    inspector.dispatch(InspectorAction::Insert("é界".into()));
    inspector.dispatch(InspectorAction::Cursor {
        right: false,
        select: false,
    });
    inspector.dispatch(InspectorAction::Insert("x".into()));
    assert_eq!(inspector.query(), "éx界");
    for text in [
        "a".repeat(513),
        "界".repeat(129),
        "unsafe\ntext".into(),
        "\u{1b}[31m".into(),
    ] {
        inspector.dispatch(InspectorAction::Insert(text));
        assert_eq!(inspector.query(), "éx界");
        assert!(inspector.message.contains("Filter refused"));
    }
    inspector.dispatch(InspectorAction::SelectAll);
    inspector.dispatch(InspectorAction::Insert("pr metadata".into()));
    assert_eq!(inspector.query(), "pr metadata");
    assert_eq!(inspector.selected_id(), Some("pr-metadata"));
}

#[test]
fn gh_provider_inspector_keyboard_and_mouse_rows_close_and_hovered_scroll_share_actions() {
    let mut keyboard = CapabilityInspector::new(None, None).unwrap();
    let (hits, _) = render(&mut keyboard, 120, 40, false);
    let mut mouse = keyboard.clone();
    let row_hit = hits
        .iter()
        .find(|hit| hit.action == InspectorAction::Row(1))
        .unwrap();
    assert_eq!(
        keyboard.key(KeyCode::Down, KeyModifiers::NONE),
        Some(InspectorAction::Move(1))
    );
    keyboard.dispatch(InspectorAction::Move(1));
    let click = mouse
        .mouse(
            MouseEventKind::Down(MouseButton::Left),
            row_hit.rect.x,
            row_hit.rect.y,
            &hits,
        )
        .unwrap();
    mouse.dispatch(click);
    assert_eq!(keyboard.selected_id(), mouse.selected_id());
    assert_eq!(keyboard.details(), mouse.details());
    assert_eq!(
        mouse.mouse(
            MouseEventKind::ScrollDown,
            mouse.list_area.x,
            mouse.list_area.y,
            &hits
        ),
        Some(InspectorAction::Move(1))
    );
    assert_eq!(
        mouse.mouse(
            MouseEventKind::ScrollDown,
            mouse.detail_area.x,
            mouse.detail_area.y,
            &hits
        ),
        Some(InspectorAction::DetailScroll(1))
    );
    assert!(mouse
        .mouse(MouseEventKind::ScrollDown, 0, 0, &hits)
        .is_none());
    let close = hits
        .iter()
        .find(|hit| hit.action == InspectorAction::Close)
        .unwrap();
    assert_eq!(
        keyboard.key(KeyCode::Esc, KeyModifiers::NONE),
        Some(InspectorAction::Close)
    );
    assert_eq!(
        mouse.mouse(
            MouseEventKind::Down(MouseButton::Left),
            close.rect.x,
            close.rect.y,
            &hits
        ),
        Some(InspectorAction::Close)
    );
    assert!(keyboard.dispatch(InspectorAction::Close));
    assert!(mouse.dispatch(InspectorAction::Close));
}

#[test]
fn gh_provider_inspector_narrow_resize_no_color_preserves_selection_focus_and_visible_hit_bounds() {
    let mut inspector = CapabilityInspector::new(None, None).unwrap();
    select(&mut inspector, "actions-runners");
    inspector.dispatch(InspectorAction::FocusNext(false));
    assert_eq!(inspector.focus, Focus::Details);
    for (width, height) in [(120, 40), (80, 24), (120, 40)] {
        let (hits, text) = render(&mut inspector, width, height, true);
        assert_eq!(inspector.selected_id(), Some("actions-runners"));
        assert_eq!(inspector.focus, Focus::Details);
        assert!(text.contains("Runner/group status"));
        assert!(text.contains("Capabilities • read-only inventory"));
        for hit in hits {
            assert!(!hit.rect.is_empty());
            assert!(hit.rect.right() <= width && hit.rect.bottom() <= height);
        }
    }
    let (hits, text) = render(&mut inspector, 20, 6, true);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].action, InspectorAction::Close);
    assert!(text.contains("Resize to inspect"));
    assert_eq!(
        inspector.list_area,
        Rect::default(),
        "Old hit rectangles cannot survive resize"
    );
    assert_eq!(
        inspector.mouse(
            MouseEventKind::Down(MouseButton::Left),
            hits[0].rect.x,
            hits[0].rect.y,
            &hits
        ),
        Some(InspectorAction::Close)
    );
    assert_eq!(
        inspector.key(KeyCode::Esc, KeyModifiers::NONE),
        Some(InspectorAction::Close)
    );
}

#[test]
fn gh_provider_inspector_untrusted_inventory_fields_are_bounded_sanitized_and_not_authority() {
    let display =
        "Implementation: TASK-14; static inventory records transport present. Unverified αβ 界!";
    let rows = prepared_lines(display, 64);
    assert_eq!(rows.concat(), display);
    assert!(rows
        .iter()
        .all(|row| crate::editor::display_width(row) <= 64));
    assert!(
        rows.iter().any(|row| row.contains("transport")),
        "A normal word must not be split into invented spaces"
    );
    let long = "é".repeat(130);
    let rows = prepared_lines(&long, 64);
    assert_eq!(rows.concat(), long);
    assert_eq!(rows.len(), 3);
    let mut inventory = capability_inventory();
    inventory[0].label = "unsafe\u{1b}[31m label\nsecond line".into();
    let inspector = CapabilityInspector::prepare(inventory.clone(), None, None).unwrap();
    assert!(!inspector.rows[0].label.contains('\u{1b}'));
    assert!(!inspector.rows[0].label.contains('\n'));
    inventory[0].transport = "x".repeat(MAX_FIELD_BYTES + 1);
    assert!(CapabilityInspector::prepare(inventory, None, None)
        .unwrap_err()
        .contains("text budget"));
    let mut inventory = capability_inventory();
    inventory[1].id = inventory[0].id.clone();
    assert!(CapabilityInspector::prepare(inventory, None, None)
        .unwrap_err()
        .contains("duplicate action identity"));
    let mut inventory = capability_inventory();
    inventory[0].id = "injected\u{1b}[31m identity".into();
    assert!(CapabilityInspector::prepare(inventory, None, None)
        .unwrap_err()
        .contains("malformed canonical action identity"));
    let inventory = (0..MAX_ROWS + 1)
        .map(|_| capability_inventory().remove(0))
        .collect();
    assert!(CapabilityInspector::prepare(inventory, None, None)
        .unwrap_err()
        .contains("row budget"));
}
