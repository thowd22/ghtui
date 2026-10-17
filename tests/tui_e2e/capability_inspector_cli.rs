//! Actual readonly inspector input; numeric syntax facts never grant API access.
use super::*;

#[derive(Clone, Copy)]
enum Build {
    Old,
    Current,
    Custom,
}
impl Build {
    fn line(self) -> &'static str {
        match self {
            Self::Old => "gh version 2.12.0\n",
            Self::Current => "gh version 2.101.0\n",
            Self::Custom => "gh version 2.101.0-custom\n",
        }
    }
    fn expected(self) -> &'static str {
        match self {
            Self::Old => "below documented flag floor",
            Self::Current => "numeric release shape meets documented flag floor",
            Self::Custom => "custom/prerelease build; flag support unverified",
        }
    }
}
fn protocol(build: Build) -> Vec<GhStep> {
    let version = || GhStep::read(&["--version"], build.line());
    let mut steps = vec![version(), user(HOST), version(), user(HOST)];
    for qualifier in ["author", "assignee", "review-requested"] {
        steps.push(GhStep::read(
            &[
                "api",
                "search/issues",
                "--hostname",
                HOST,
                "--method",
                "GET",
                "-f",
                &format!("q=is:pr is:open {qualifier}:fixture"),
                "-f",
                "per_page=100",
                "-f",
                "page=1",
            ],
            "{\"total_count\":0,\"incomplete_results\":false,\"items\":[]}",
        ));
    }
    steps
}
fn assert_protocol(session: &Session, fixture: &Fixture, build: Build) {
    let actual = requests(session, fixture);
    let expected = protocol(build);
    assert_eq!(actual.len(), 7);
    for (row, step) in actual.iter().zip(expected) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["stdin_hex"], "");
    }
}
fn inbox_frame(fixture: &Fixture) -> ghtui::storage::SyncFrame {
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    cache
        .sync_frame(
            &Scope::github(HOST, Some(42)).unwrap(),
            &format!(
                "personal-inbox:{}",
                json!({"repositories":[],"excluded":[]})
            ),
        )
        .unwrap()
        .expect("actually fetched empty inbox frame")
}
fn journey(build: Build, mouse: bool) {
    let fixture = fixture(Scenario::AuthDenied);
    fixture.scripted_gh(&protocol(build));
    let git_before = git_snapshot(&fixture);
    let config_before = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, Scenario::AuthDenied);
    session.wait_for_screen("Recent commits — saved versions");
    activate(&mut session, mouse, "1 Inbox", b"1");
    session.assert_focused_view("Inbox");
    activate(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("0 unique PRs across 0 queue matches • complete");
    assert_protocol(&session, &fixture, build);
    let frame_before = inbox_frame(&fixture);
    let requests_before = session.inspect_command_journal(&fixture);
    let trace_before = trace(&fixture);
    assert!(!trace_before.is_empty());
    for (index, (id, label, floor, flag)) in [
        ("pr-details", "PR summary, timeline", "2.48.0", "--slurp"),
        ("pr-merge", "Merge, squash", "2.13.0", "--match-head-commit"),
        ("pr-lifecycle", "Draft, ready", "2.16.0", "--undo"),
        (
            "actions-logs",
            "Bounded run/job logs",
            "2.26.0",
            "--attempt",
        ),
        (
            "actions-secrets-variables",
            "Secret metadata and variables",
            "2.75.0",
            "--no-repos-selected",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        open(&mut session, mouse);
        filter(&mut session, mouse, id);
        session.wait_for_screen("1 matching");
        details(&mut session, mouse, label, id);
        scroll_detail_until(&mut session, mouse, "CLI provider route only:");
        scroll_detail_until(
            &mut session,
            mouse,
            &format!("Documented local flag floor: gh {floor}"),
        );
        scroll_detail_until(&mut session, mouse, flag);
        scroll_detail_until(&mut session, mouse, build.expected());
        session.resize(80, 24);
        // Resize may reset the prepared detail viewport. Reacquire a visible
        // origin instead of assuming its previous scroll offset survived.
        scroll_detail_up_until(&mut session, mouse, &format!("Action: {label}"));
        scroll_detail_until(&mut session, mouse, "Documented local flag floor:");
        scroll_detail_until(&mut session, mouse, build.expected());
        if mouse && index == 1 {
            session.capture_completed_terminal_v1(match build {
                Build::Old => "cli-inventory-old-80x24",
                Build::Current => "cli-inventory-current-80x24",
                Build::Custom => "cli-inventory-custom-80x24",
            });
        }
        scroll_detail_until(&mut session, mouse, "current target permissions");
        scroll_detail_until(&mut session, mouse, &format!("releases/tag/v{floor}"));
        assert!(!session.screen().contains(PRIVATE_CANARY));
        close(&mut session, mouse, "Inbox");
        assert_eq!(session.inspect_command_journal(&fixture), requests_before);
        assert_eq!(trace(&fixture), trace_before);
        assert_eq!(inbox_frame(&fixture), frame_before);
        session.resize(120, 40);
    }
    activate(&mut session, mouse, "2 Repository", b"2");
    session.assert_focused_view("Repository");
    activate(&mut session, mouse, "[w Working]", b"w");
    session.wait_for_screen("dirty local text must remain unchanged");
    assert_eq!(git_snapshot(&fixture), git_before);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config_before
    );
    assert_storage(&fixture, Scenario::AuthDenied);
    session.quit_and_assert_restored_terminal(mouse);
    assert_protocol(&session, &fixture, build);
    assert_eq!(inbox_frame(&fixture), frame_before);
    assert_eq!(git_snapshot(&fixture), git_before);
    assert_storage(&fixture, Scenario::AuthDenied);
}

#[test]
fn gh_provider_cli_inventory_keyboard_old_flag_floors_never_grant_host_permission() {
    journey(Build::Old, false);
}
#[test]
fn gh_provider_cli_inventory_mouse_old_flag_floors_never_grant_host_permission() {
    journey(Build::Old, true);
}
#[test]
fn gh_provider_cli_inventory_keyboard_current_shape_is_local_syntax_not_native_parity() {
    journey(Build::Current, false);
}
#[test]
fn gh_provider_cli_inventory_mouse_current_shape_is_local_syntax_not_native_parity() {
    journey(Build::Current, true);
}
#[test]
fn gh_provider_cli_inventory_keyboard_custom_build_keeps_flag_support_unknown() {
    journey(Build::Custom, false);
}
#[test]
fn gh_provider_cli_inventory_mouse_custom_build_keeps_flag_support_unknown() {
    journey(Build::Custom, true);
}
