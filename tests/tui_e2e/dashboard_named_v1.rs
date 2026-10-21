//! TASK-62 named saved dashboards on the actual executable: a stale threshold chosen in
//! Layout, two named dashboards created from the Dashboards list (N), a switch that freshly
//! reads the saved scope, a rename and a confirmed delete, then a restart that reopens the
//! active one. Preferences live in the version-2 saved view; the version-1 default is never
//! written. 160 columns (Dashboards through Commands, its list controls on the bar) and 80x24.
use super::*;
use ghtui::dashboard_profile_v1::{PROFILES_KEY, PROFILE_KEY};

fn saved_views(fixture: &Fixture) -> (Option<Value>, Option<Value>) {
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let scope = Scope::github(gh_fixture::HOST, Some(42)).unwrap();
    (
        cache.saved_view(&scope, PROFILES_KEY).unwrap(),
        cache.saved_view(&scope, PROFILE_KEY).unwrap(),
    )
}
/// The saved dashboards as (name, window days, stale days), and the active name.
fn saved(fixture: &Fixture) -> (Vec<(String, u64, u64)>, Value) {
    let (profiles, legacy) = saved_views(fixture);
    assert!(legacy.is_none(), "the version-1 default is never written");
    let profiles = profiles.expect("version-2 dashboards saved");
    assert_eq!(profiles["kind"], "ghtui-dashboards");
    assert_eq!(profiles["version"], 2);
    assert_eq!(profiles["account_id"], 42);
    assert!(profiles.get("cached_metrics").is_none());
    let boards = profiles["dashboards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|board| {
            assert_eq!(board["repositories"], json!([APP]));
            (
                board["name"].as_str().unwrap().to_owned(),
                board["window_days"].as_u64().unwrap(),
                board["stale_days"].as_u64().unwrap(),
            )
        })
        .collect();
    (boards, profiles["active"].clone())
}
fn board(name: &str, window: u64, stale: u64) -> (String, u64, u64) {
    (name.into(), window, stale)
}
/// The Cards view (not the Dashboards list) on a completed frame, so the next click reads
/// the Cards key bar rather than the list's, whose tokens sit elsewhere.
fn cards_shown(session: &mut Session) {
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            !text.contains("Saved dashboards • Enter")
                && text.contains("METRIC ")
                && crate::support::locate_control(frame, "[n New]").is_none()
                && crate::support::locate_control(frame, "[0 30 days]").is_some()
        },
        "Cards view and its key bar after leaving the Dashboards list",
        Duration::from_secs(8),
    );
}
/// Dashboards is the lowest-priority control: by key, or through Commands for the mouse.
fn open_dashboards(session: &mut Session, mouse: bool) {
    if mouse {
        session.invoke_via_palette(true, "[N Dashboards]");
    } else {
        session.send_key(b"N");
    }
    session.wait_for_screen("Saved dashboards • Enter or click switches");
}
/// Cycles the Layout stale-threshold row once, from `from` days.
fn cycle_stale(session: &mut Session, mouse: bool, from: u64) {
    let row = format!("Stale threshold: {from} days");
    if mouse {
        session.click_visible_action(&row);
    } else {
        session.send_key(b"\x1b[F");
        selected(session, &row);
        session.send_key(b" ");
    }
}
fn set_stale(session: &mut Session, mouse: bool, path: &[u64]) {
    choose(session, mouse, "[L Layout]", b"L");
    session.wait_for_screen("Layout only");
    for pair in path.windows(2) {
        cycle_stale(session, mouse, pair[0]);
        session.wait_for_screen(&format!("Stale threshold: {} days", pair[1]));
    }
    back(session, mouse);
    session.wait_for_screen(&format!(
        "idle {}+ days",
        path.last().copied().unwrap_or_default()
    ));
}
/// New dashboard from the list: `n New`, the typed name, then Save name.
fn create(session: &mut Session, fixture: &Fixture, mouse: bool, name: &str, count: usize) {
    choose(session, mouse, "[n New]", b"n");
    session.wait_for_screen("New dashboard name: ");
    session.wait_for_control("[Enter Save name]");
    session.send_key(name.as_bytes());
    session.wait_for_screen(&format!("New dashboard name: {name}"));
    choose(session, mouse, "[Enter Save name]", b"\r");
    session.wait_for_screen(&format!("New dashboard saved: {name}"));
    journal(session, fixture, count);
    session.wait_for_screen("Saved dashboards • Enter or click switches");
}

#[test]
fn dashboard_named_keyboard_create_switch_rename_delete_and_restart_restore() {
    named_dashboards(false);
}
#[test]
fn dashboard_named_mouse_create_switch_rename_delete_and_restart_restore() {
    named_dashboards(true);
}
fn named_dashboards(mouse: bool) {
    let fixture = configured(false);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let scope = Scope::github(gh_fixture::HOST, Some(42)).unwrap();
    Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3"))
        .unwrap()
        .put_draft(&scope, "named-dashboards", "keep exact draft bytes")
        .unwrap();
    let mut steps = refresh(&[APP]);
    steps.extend(window(&[APP])); // Create Team reverifies the original source.
    steps.extend(window(&[APP])); // 30-day window.
    steps.extend(window(&[APP])); // Create Ops.
    steps.extend(refresh(&[APP])); // Switch to Team freshly reads its saved scope.
    steps.extend(gh_fixture::account(42)); // Rename: preferences only, principal bracket.
    steps.extend(gh_fixture::account(42));
    steps.extend(gh_fixture::account(42)); // Delete: preferences only, principal bracket.
    steps.extend(gh_fixture::account(42));
    steps.extend(refresh(&[APP])); // New executable.
    steps.extend(refresh(&[APP])); // Open saved restores the active dashboard.
    assert_eq!(steps.len(), 54);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 160, 42);
    start(&mut session, mouse);
    journal(&mut session, &fixture, 7);
    cards_ready(&mut session, 1, 1);
    // Both attention widgets are cards of the default layout.
    session.wait_for_screen("idle 7+ days");
    session.wait_for_screen("Not read • Enter reads current threads");
    assert_eq!(saved_views(&fixture), (None, None));

    // Team: stale threshold 30 days, 7-day window.
    set_stale(&mut session, mouse, &[7, 14, 30]);
    open_dashboards(&mut session, mouse);
    session.wait_for_screen("No saved dashboards");
    session.assert_control_absent("[e Rename]");
    create(&mut session, &fixture, mouse, "Team", 13);
    let (boards, active) = saved(&fixture);
    assert_eq!(boards, [board("Team", 7, 30)]);
    assert_eq!(active, "Team");
    session.wait_for_screen("Team • active • 1 repositories • 7 days • stale 30d");

    // Ops: 30-day window, stale threshold 3 days.
    back(&mut session, mouse);
    cards_shown(&mut session);
    choose(&mut session, mouse, "[0 30 days]", b"0");
    session.wait_for_screen("Dashboard UTC window prepared");
    journal(&mut session, &fixture, 19);
    set_stale(&mut session, mouse, &[30, 3]);
    open_dashboards(&mut session, mouse);
    create(&mut session, &fixture, mouse, "Ops", 25);
    let (boards, active) = saved(&fixture);
    assert_eq!(boards, [board("Team", 7, 30), board("Ops", 30, 3)]);
    assert_eq!(active, "Ops");
    session.wait_for_screen("Team • 1 repositories • 7 days • stale 30d");
    session.wait_for_screen("Ops • active • 1 repositories • 30 days • stale 3d");
    selected(&mut session, "Ops • active");

    // Switch to Team: a fresh read of its saved scope, its window and threshold.
    if mouse {
        session.click_visible_action("Team • 1 repositories");
    } else {
        session.send_key(b"\x1b[A");
        selected(&mut session, "Team • 1 repositories");
        session.send_key(b"\r");
    }
    session.wait_for_screen("Switched to saved dashboard Team");
    journal(&mut session, &fixture, 32);
    session.wait_for_screen("Saved scope");
    session.wait_for_screen("idle 30+ days");
    let clock = as_of(&mut session);
    assert_window(&mut session, 7, clock);
    let (boards, active) = saved(&fixture);
    assert_eq!(boards, [board("Team", 7, 30), board("Ops", 30, 3)]);
    assert_eq!(active, "Team");

    // Rename the active Team: preferences only, no repository read.
    open_dashboards(&mut session, mouse);
    selected(&mut session, "Team • active");
    choose(&mut session, mouse, "[e Rename]", b"e");
    session.wait_for_screen("Rename Team to: Team");
    session.send_key(b" A");
    session.wait_for_screen("Rename Team to: Team A");
    choose(&mut session, mouse, "[Enter Save name]", b"\r");
    session.wait_for_screen("Renamed saved dashboard Team → Team A");
    journal(&mut session, &fixture, 36);
    session.wait_for_screen("Team A • active • 1 repositories");
    let (boards, active) = saved(&fixture);
    assert_eq!(boards, [board("Team A", 7, 30), board("Ops", 30, 3)]);
    assert_eq!(active, "Team A");

    // Delete Ops: the first x only arms; the second confirms.
    if mouse {
        let (x, y) = session.locate("Team A • active").unwrap();
        session.wheel(x, y, true);
    } else {
        session.send_key(b"\x1b[B");
    }
    selected(&mut session, "Ops • 1 repositories");
    choose(&mut session, mouse, "[x Delete]", b"x");
    session.wait_for_screen("Delete saved dashboard Ops? x again confirms");
    session.wait_for_control("[x Confirm delete]");
    journal(&mut session, &fixture, 36);
    assert_eq!(saved(&fixture).0.len(), 2);
    choose(&mut session, mouse, "[x Confirm delete]", b"x");
    session.wait_for_screen("Deleted saved dashboard Ops");
    journal(&mut session, &fixture, 40);
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Ops • 1 repositories"),
            "deleted dashboard row gone",
            Duration::from_secs(8),
        )
        .unwrap();
    let (boards, active) = saved(&fixture);
    assert_eq!(boards, [board("Team A", 7, 30)]);
    assert_eq!(active, "Team A");

    // 80x24: the list and its controls stay reachable; Dashboards again through Commands.
    session.resize(80, 24);
    session.wait_for_screen("Team A • active");
    session.wait_for_control("[n New]");
    back(&mut session, mouse);
    cards_shown(&mut session);
    session.wait_for_screen("idle 30+ days");
    open_dashboards(&mut session, mouse);
    session.wait_for_screen("Team A • active");
    back(&mut session, mouse);
    cards_shown(&mut session);
    session.wait_for_screen("Needs my review");
    journal(&mut session, &fixture, 40);
    session.quit_and_assert_restored_terminal(mouse);
    preserve(&fixture, &head, &index, &status);

    // A new executable opens the active saved dashboard only when asked.
    let mut session = Session::launch(&fixture, 160, 42);
    start(&mut session, mouse);
    cards_ready(&mut session, 1, 1);
    session.wait_for_screen("idle 7+ days");
    journal(&mut session, &fixture, 47);
    choose(&mut session, mouse, "[O Open saved]", b"O");
    session.wait_for_screen("Saved dashboard restored: Team A");
    journal(&mut session, &fixture, 54);
    session.wait_for_screen("idle 30+ days");
    let clock = as_of(&mut session);
    assert_window(&mut session, 7, clock);
    session.quit_and_assert_restored_terminal(mouse);
    journal(&mut session, &fixture, 54);
    let (boards, active) = saved(&fixture);
    assert_eq!(boards, [board("Team A", 7, 30)]);
    assert_eq!(active, "Team A");
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    assert_eq!(
        cache.draft(&scope, "named-dashboards").unwrap().as_deref(),
        Some("keep exact draft bytes")
    );
    assert!(cache
        .saved_views(&Scope::github(gh_fixture::HOST, Some(43)).unwrap())
        .unwrap()
        .is_empty());
    metadata_only(&fixture);
    preserve(&fixture, &head, &index, &status);
}
