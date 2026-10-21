//! Real read-only inspector journeys, not permission/native-parity grants.
use super::*;

fn capture_completed_text(session: &mut Session, text: &str, label: &str) {
    let wanted = compact(text);
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_screen().is_some_and(|frame| {
            crate::support::overlay_body(frame, TITLE)
                .is_some_and(|body| compact(&body).contains(&wanted))
        }) {
            break;
        }
        if std::time::Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "Completed owned inspector cells missing {text:?}; artifacts: {}",
                session.artifact_dir.display()
            );
        }
    }
    // The pinned backend's Show/Hide follows all cell writes. This is not a
    // cursorless/caret-position claim and does not inspect application state.
    session.capture_completed_terminal_v1(label);
}

fn journey_contracts(mouse: bool) {
    let fixture = fixture(Scenario::AuthDenied);
    fixture.scripted_gh(&protocol(Build::Current));
    let git_before = git_snapshot(&fixture);
    let config_before = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, Scenario::AuthDenied);
    session.wait_for_screen("Recent commits");
    activate(&mut session, mouse, "1 Inbox", b"1");
    // Each view has its own key bar: take Refresh from a completed Inbox frame, never from
    // the Repository bar that was painted before the switch.
    session.wait_for_cursorless_condition(
        |frame| {
            frame
                .contents()
                .contains(&crate::support::controls::view_title("Inbox"))
                && crate::support::main_footer_visible(frame)
        },
        "completed Inbox frame and key bar",
        std::time::Duration::from_secs(8),
    );
    activate(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("0 unique PRs across 0 queue matches • complete");
    assert_protocol(&session, &fixture, Build::Current);
    let frame_before = inbox_frame(&fixture);
    let calls = session.inspect_command_journal(&fixture);
    let trace_before = trace(&fixture);
    for (id, label, row, operation, effect, task) in [
        (
            "pr-metadata",
            "Title, body, base",
            "pr.metadata",
            "add/remove with item readback",
            "May write remote state",
            "TASK-18",
        ),
        (
            "pr-pending-reviews",
            "Persistent pending reviews",
            "pr.pending-review",
            "submit/discard",
            "May write remote state",
            "TASK-20",
        ),
        (
            "pr-suggestions",
            "Preview and apply suggested",
            "pr.suggestions",
            "selected edits",
            "Isolated Git changes",
            "TASK-23",
        ),
        (
            "pr-merge",
            "Merge, squash",
            "pr.merge",
            "allowed methods/rules/checks",
            "May write remote state",
            "TASK-24",
        ),
        (
            "actions-details",
            "Run attempts, matrix",
            "actions.details",
            "matrix jobs",
            "Read-only inspection",
            "TASK-53",
        ),
        (
            "actions-dispatch",
            "Workflow YAML",
            "actions.dispatch",
            "typed workflow_dispatch inputs",
            "Mixed read/write family",
            "TASK-54",
        ),
        (
            "actions-control",
            "Rerun all/failed",
            "actions.control",
            "specific jobs",
            "May write remote state",
            "TASK-55",
        ),
        (
            "actions-artifacts",
            "Artifact inventory",
            "actions.artifacts",
            "untrusted archive content",
            "Mixed read/write family",
            "TASK-56",
        ),
        (
            "actions-secrets-variables",
            "Secret metadata and variables",
            "actions.settings",
            "Existing secret values cannot be read",
            "May write remote state",
            "TASK-59",
        ),
        (
            "actions-runners",
            "Runner/group status",
            "actions.runners",
            "no runner service installation",
            "May write remote state",
            "TASK-60",
        ),
    ] {
        open(&mut session, mouse);
        filter(&mut session, mouse, id);
        session.wait_for_screen("1 matching");
        details(&mut session, mouse, label, id);
        scroll_detail_until(&mut session, mouse, "Contract scope:");
        scroll_detail_until(&mut session, mouse, row);
        scroll_detail_until(&mut session, mouse, "Required native operations:");
        scroll_detail_until(&mut session, mouse, operation);
        if mouse && matches!(id, "pr-metadata" | "actions-runners") {
            capture_completed_text(
                &mut session,
                operation,
                if id == "pr-metadata" {
                    "inventory-pr-requirements-120x40"
                } else {
                    "inventory-runner-requirements-120x40"
                },
            );
        }
        scroll_detail_until(&mut session, mouse, effect);
        scroll_detail_until(&mut session, mouse, "Required target guard:");
        scroll_detail_until(&mut session, mouse, "Verification tasks:");
        scroll_detail_until(&mut session, mouse, task);
        scroll_detail_until(&mut session, mouse, "Host limit:");
        scroll_detail_until(&mut session, mouse, "Evidence limit:");
        session.resize(80, 24);
        scroll_detail_up_until(&mut session, mouse, &format!("Action: {label}"));
        scroll_detail_until(&mut session, mouse, "Contract scope:");
        scroll_detail_until(&mut session, mouse, row);
        scroll_detail_until(&mut session, mouse, "Evidence limit:");
        assert!(!session.screen().contains(PRIVATE_CANARY));
        if mouse && id == "actions-secrets-variables" {
            capture_completed_text(
                &mut session,
                "Evidence limit:",
                "inventory-secret-limits-80x24",
            );
        }
        close(&mut session, mouse, "Inbox");
        assert_eq!(session.inspect_command_journal(&fixture), calls);
        assert_eq!(trace(&fixture), trace_before);
        assert_eq!(inbox_frame(&fixture), frame_before);
        session.resize(120, 40);
    }
    activate(&mut session, mouse, "2 Repository", b"2");
    activate(&mut session, mouse, "[w Working]", b"w");
    session.wait_for_screen("dirty local text must remain unchanged");
    assert_eq!(git_snapshot(&fixture), git_before);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config_before
    );
    assert_storage(&fixture, Scenario::AuthDenied);
    session.quit_and_assert_restored_terminal(mouse);
    assert_protocol(&session, &fixture, Build::Current);
    assert_eq!(inbox_frame(&fixture), frame_before);
    assert_eq!(git_snapshot(&fixture), git_before);
}

#[test]
fn gh_provider_inventory_contracts_keyboard_exact_requirements_and_limits_keep_all_effects_unchanged(
) {
    journey_contracts(false);
}
#[test]
fn gh_provider_inventory_contracts_mouse_exact_requirements_and_limits_keep_all_effects_unchanged()
{
    journey_contracts(true);
}
