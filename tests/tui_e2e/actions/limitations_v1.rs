//! TASK-58/59/60: denied, missing or out-of-scope settings reads are explained on screen and
//! their mutation controls stay disabled (by key and by click) without any request; host
//! cache limits read on Usage, and a refresh reads them again. Strict offline fixtures,
//! keyboard and mouse journeys, no Git changes.
use super::*;

fn denied(endpoint: &str, pages: bool) -> GhStep {
    let mut step = read(endpoint, Value::Null, pages);
    step.status = 1;
    step.stdout.clear();
    step.stderr = b"gh: Resource not accessible by integration (HTTP 403)".to_vec();
    step
}
fn open_actions(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    choose(session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
}
/// A disabled control explains itself in a dialog and sends nothing.
fn inspect_disabled(session: &mut Session, mouse: bool, label: &str, reason: &str) {
    command(session, mouse, label);
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen(reason);
    assert!(!session.screen().contains("ACTIONS INTENT"));
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b");
    }
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "Disabled control explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
}
fn no_writes_and_clean(rows: &[Value], fixture: &Fixture) {
    assert!(!rows.iter().any(
        |row| row["args"]
            .as_array()
            .is_some_and(|args| args.iter().any(|arg| matches!(
                arg.as_str(),
                Some("POST" | "PUT" | "PATCH" | "DELETE" | "set" | "delete")
            )))
    ));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
}

#[test]
fn actions_cache_keyboard_usage_explains_denied_host_limits_then_refresh_reads_them() {
    cache_limits(false)
}
#[test]
fn actions_cache_mouse_usage_explains_denied_host_limits_then_refresh_reads_them() {
    cache_limits(true)
}
fn cache_limits(mouse: bool) {
    let fixture = fixture();
    let usage = json!({"active_caches_count":2,"active_caches_size_in_bytes":6144});
    let mut steps = initial();
    steps.extend([
        read(
            "repos/example/repo/actions/cache/usage",
            usage.clone(),
            false,
        ),
        denied("repos/example/repo/actions/cache/storage-limit", false),
    ]);
    let denied_count = steps.len();
    steps.extend(current_account());
    steps.extend([
        read("repos/example/repo/actions/cache/usage", usage, false),
        read(
            "repos/example/repo/actions/cache/storage-limit",
            json!({"max_cache_size_gb":10}),
            false,
        ),
        read(
            "repos/example/repo/actions/cache/retention-limit",
            json!({"max_cache_retention_days":7}),
            false,
        ),
    ]);
    let refreshed_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_actions(&mut session, mouse);
    choose(&mut session, mouse, "Usage", b"u");
    session.wait_for_screen("Cache storage: 6144 bytes in 2 caches");
    session.wait_for_screen("Cache limits unavailable: GitHub denied");
    assert!(!session.screen().contains("Host cache limits:"));
    let rows = journal(&mut session, &fixture, denied_count);
    no_writes_and_clean(&rows, &fixture);
    if mouse {
        session.click_visible_action("[r Refresh]");
    } else {
        session.send_key(b"r");
    }
    session.wait_for_screen("Host cache limits: storage up to 10 GB • retention up to 7 days");
    assert!(!session.screen().contains("Cache limits unavailable"));
    let rows = journal(&mut session, &fixture, refreshed_count);
    no_writes_and_clean(&rows, &fixture);
    session.resize(80, 24);
    session.wait_for_screen("Host cache limits: storage up to 10 GB");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_settings_keyboard_runner_groups_need_organization_and_denied_runners_disable_controls() {
    settings_limits(false)
}
#[test]
fn actions_settings_mouse_runner_groups_need_organization_and_denied_runners_disable_controls() {
    settings_limits(true)
}
fn settings_limits(mouse: bool) {
    let fixture = fixture();
    let mut steps = initial();
    let opened = steps.len();
    steps.push(denied(
        "repos/example/repo/actions/runners?per_page=100",
        true,
    ));
    let runners_denied = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_actions(&mut session, mouse);
    // A repository scope cannot list runner groups: the page says so and sends nothing.
    choose(&mut session, mouse, "Runner groups", b"g");
    session.wait_for_screen("Runner groups belong to an organization");
    journal(&mut session, &fixture, opened);
    inspect_disabled(
        &mut session,
        mouse,
        "Create runner group",
        "Runner groups belong to an",
    );
    journal(&mut session, &fixture, opened);
    // A denied runner read explains the admin requirement; label and removal stay disabled.
    choose(&mut session, mouse, "Runners", b"n");
    session.wait_for_screen("Self-hosted runners unavailable: GitHub denied this account");
    assert!(!session
        .screen()
        .contains("No records reported in this scope."));
    journal(&mut session, &fixture, runners_denied);
    inspect_disabled(
        &mut session,
        mouse,
        "Remove selected runner",
        "Self-hosted runners unavailable",
    );
    let rows = journal(&mut session, &fixture, runners_denied);
    no_writes_and_clean(&rows, &fixture);
    session.resize(80, 24);
    session.wait_for_screen("Self-hosted runners unavailable");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_secrets_vars_keyboard_denied_metadata_read_explains_and_disables_set_secret() {
    secrets_denied(false)
}
#[test]
fn actions_secrets_vars_mouse_denied_metadata_read_explains_and_disables_set_secret() {
    secrets_denied(true)
}
fn secrets_denied(mouse: bool) {
    let fixture = fixture();
    let mut steps = initial();
    steps.push(denied(
        "repos/example/repo/actions/secrets?per_page=100",
        true,
    ));
    let denied_count = steps.len();
    steps.extend(current_account());
    steps.push(read(
        "repos/example/repo/actions/secrets?per_page=100",
        json!([{"secrets":[{"name":"DEPLOY_TOKEN","created_at":"2026-10-13T12:00:00Z","updated_at":"2026-10-13T12:00:00Z"}]}]),
        true,
    ));
    let refreshed_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_actions(&mut session, mouse);
    choose(&mut session, mouse, "Secrets", b"S");
    session.wait_for_screen("Secret metadata unavailable: GitHub denied this account");
    assert!(!session
        .screen()
        .contains("No records reported in this scope."));
    journal(&mut session, &fixture, denied_count);
    inspect_disabled(
        &mut session,
        mouse,
        "Set secret",
        "Secret metadata unavailable",
    );
    journal(&mut session, &fixture, denied_count);
    // Once access is granted, a refresh lists the metadata and clears the limitation.
    if mouse {
        session.click_visible_action("[r Refresh]");
    } else {
        session.send_key(b"r");
    }
    session.wait_for_screen("DEPLOY_TOKEN • updated 2026-10-13T12:00:00Z");
    assert!(!session.screen().contains("Secret metadata unavailable"));
    let rows = journal(&mut session, &fixture, refreshed_count);
    no_writes_and_clean(&rows, &fixture);
    session.quit_and_assert_restored_terminal(mouse);
}
