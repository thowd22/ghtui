//! Actual executable/PTY journeys for saved preferences, not restored metrics.
use super::*;
use ghtui::dashboard_profile_v1::PROFILE_KEY;
use rusqlite::{params, Connection};

fn database(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("config/ghtui/cache.sqlite3")
}
fn saved(fixture: &Fixture) -> Value {
    let sql = Connection::open(database(fixture)).unwrap();
    let scope = Scope::github(gh_fixture::HOST, Some(42)).unwrap();
    let raw: String = sql
        .query_row(
            "SELECT payload FROM saved_views WHERE host=?1 AND login=?2 AND name=?3",
            params![scope.host, scope.login, PROFILE_KEY],
            |row| row.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}
fn absent(session: &mut Session, text: &str) {
    session
        .try_wait_for_condition(
            |screen| !screen.contains(text),
            "hidden dashboard widget",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn exit(session: &mut Session, mouse: bool) {
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn dashboards_keyboard_save_restore_restart_exact_drilldown_and_explicit_defaults() {
    happy_dashboard(false);
}
#[test]
fn dashboards_mouse_save_restore_restart_exact_drilldown_and_explicit_defaults() {
    happy_dashboard(true);
}
fn happy_dashboard(mouse: bool) {
    let fixture = configured(true);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let scope = Scope::github(gh_fixture::HOST, Some(42)).unwrap();
    {
        let cache = Cache::open(&database(&fixture)).unwrap();
        cache
            .put_draft(&scope, "dashboard-unrelated", "preserve exact draft bytes")
            .unwrap();
    }
    let mut steps = refresh(&[APP, TOOLS]);
    steps.extend(refresh(&[APP]));
    steps.extend(window(&[APP])); // 30-day preference, original evidence clock.
    steps.extend(window(&[APP])); // Save verifies original account/access/source.
    steps.extend(refresh(&[APP, TOOLS])); // Explicit defaults widen scope.
    steps.extend(refresh(&[APP, TOOLS])); // New executable, no automatic restore.
    steps.extend(refresh(&[APP])); // Explicit restore freshly queries saved subset.
    steps.extend(window(&[APP]));
    steps.extend(refresh(&[APP]));
    steps.extend(open_steps());
    assert_eq!(steps.len(), 86);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 140, 42);
    start(&mut session, mouse);
    journal(&mut session, &fixture, 10);
    choose(&mut session, mouse, "[R Repositories]", b"R");
    if mouse {
        session.click_visible_action(&format!("[{APP}]"));
    } else {
        session.send_key(b"\x1b[B");
        selected(&mut session, APP);
        session.send_key(b"\r");
    }
    session.wait_for_screen(&format!("Repository: {APP}"));
    journal(&mut session, &fixture, 17);
    let clock = as_of(&mut session);
    choose(&mut session, mouse, "[0 30 days]", b"0");
    assert_window(&mut session, 30, clock);
    journal(&mut session, &fixture, 23);
    choose(&mut session, mouse, "[L Layout]", b"L");
    session.wait_for_screen("Layout only");
    choose(&mut session, mouse, "[x] Loaded open PRs", b" ");
    session.wait_for_screen("[ ] Loaded open PRs");
    if mouse {
        let (x, y) = session.locate("[ ] Loaded open PRs").unwrap();
        session.wheel(x, y, true);
    } else {
        session.send_key(b"\x1b[B");
    }
    // Mouse wheel deliberately moves three rows; use row activation twice to
    // select the same widget without retaining a visibility edit.
    if mouse {
        session.click_visible_action("[x] Needs my review");
        session.wait_for_screen("[ ] Needs my review");
        session.click_visible_action("[ ] Needs my review");
        session.wait_for_screen("[x] Needs my review");
    }
    selected(&mut session, "Needs my review");
    choose(&mut session, mouse, "[< Earlier]", b"<");
    selected(&mut session, "Needs my review");
    session.resize(80, 24);
    session.wait_for_control("[Space Show/hide]");
    back(&mut session, mouse);
    wait_card_value(&mut session, "Needs my review", "1");
    absent(&mut session, "Loaded open PRs");
    choose(&mut session, mouse, "[i Details]", b"i");
    scroll_to(&mut session, mouse, "non-draft PRs", "Unknown");
    back(&mut session, mouse);
    session.resize(140, 42);
    choose(&mut session, mouse, "[S Save]", b"S");
    session.wait_for_screen("Default dashboard saved");
    journal(&mut session, &fixture, 29);
    assert_eq!(as_of(&mut session), clock);
    let profile = saved(&fixture);
    assert_eq!(profile["repositories"], json!([APP]));
    assert_eq!(profile["window_days"], 30);
    assert_eq!(profile["account_id"], 42);
    assert_eq!(
        profile["layout"]["slots"][0],
        json!({"kind":"NeedsMyReview","visible":true})
    );
    assert_eq!(
        profile["layout"]["slots"][1],
        json!({"kind":"OpenPrs","visible":false})
    );
    assert!(profile.get("cached_metrics").is_none());
    choose(&mut session, mouse, "[D Defaults]", b"D");
    cards_ready(&mut session, 3, 1);
    journal(&mut session, &fixture, 39);
    assert_eq!(saved(&fixture), profile);
    exit(&mut session, mouse);
    preserve(&fixture, &head, &index, &status);

    let mut session = Session::launch(&fixture, 140, 42);
    start(&mut session, mouse);
    cards_ready(&mut session, 3, 1); // No automatic cached evidence restoration.
    choose(&mut session, mouse, "[O Open saved]", b"O");
    session.wait_for_screen("Saved scope");
    wait_card_value(&mut session, "Needs my review", "1");
    absent(&mut session, "Loaded open PRs");
    journal(&mut session, &fixture, 56);
    let restored_clock = as_of(&mut session);
    assert_window(&mut session, 30, restored_clock);
    choose(&mut session, mouse, "[7 7 days]", b"7");
    assert_window(&mut session, 7, restored_clock);
    journal(&mut session, &fixture, 62);
    let before_refresh = session.completed_cursorless_end();
    choose(&mut session, mouse, "[r Refresh]", b"r");
    wait_card_value(&mut session, "Needs my review", "1");
    journal(&mut session, &fixture, 69);
    session.wait_for_new_cursorless_condition(
        before_refresh,
        |frame| {
            frame
                .contents()
                .contains("Dashboard loaded • repository-wide snapshot")
                && frame.contents().contains("Dashboard • focused view")
                && crate::support::locate_control(frame, "[r Refresh]").is_some()
                && crate::support::locate_control(frame, "[Esc Cancel]").is_none()
        },
        "fresh completed restored-scope refresh before opening a card",
        Duration::from_secs(8),
    );
    if mouse {
        session.click_visible_action("Needs my review");
    } else {
        session.send_key(b"\x1b[H");
        selected(&mut session, "Needs my review");
        session.send_key(b"\r");
    }
    session.wait_for_screen(TITLE);
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .any(|line| line.contains(TITLE) && line.contains("alice"))
            },
            "contributor row with its alice actor cell",
            Duration::from_secs(8),
        )
        .unwrap();
    if mouse {
        session.click_visible_action(TITLE);
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen(BODY);
    session.wait_for_screen("@alice");
    journal(&mut session, &fixture, 86);
    exit(&mut session, mouse);
    assert_eq!(saved(&fixture), profile);
    let cache = Cache::open(&database(&fixture)).unwrap();
    assert_eq!(
        cache
            .draft(&scope, "dashboard-unrelated")
            .unwrap()
            .as_deref(),
        Some("preserve exact draft bytes")
    );
    assert!(cache
        .saved_view(
            &Scope::github(gh_fixture::HOST, Some(43)).unwrap(),
            PROFILE_KEY
        )
        .unwrap()
        .is_none());
    metadata_only(&fixture);
    preserve(&fixture, &head, &index, &status);
}

#[test]
fn dashboards_keyboard_malformed_profile_hides_old_values_and_recovers_without_overwrite() {
    refused_dashboard(false, false);
}
#[test]
fn dashboards_mouse_malformed_profile_hides_old_values_and_recovers_without_overwrite() {
    refused_dashboard(true, false);
}
#[test]
fn dashboards_keyboard_undeclared_profile_never_queries_or_expands_scope() {
    refused_dashboard(false, true);
}
#[test]
fn dashboards_mouse_undeclared_profile_never_queries_or_expands_scope() {
    refused_dashboard(true, true);
}
fn refused_dashboard(mouse: bool, outside: bool) {
    let fixture = configured(true);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let scope = Scope::github(gh_fixture::HOST, Some(42)).unwrap();
    let payload = if outside {
        json!({"kind":"ghtui-dashboard","version":1,"host":gh_fixture::HOST,"account_id":42,"repositories":["outside/never-query"],"window_days":7,"layout":{"slots":[
            {"kind":"OpenPrs","visible":true},{"kind":"NeedsMyReview","visible":true},
            {"kind":"OpenAge","visible":true},{"kind":"ReviewWait","visible":true},
            {"kind":"FirstReview","visible":true},{"kind":"MergeLatency","visible":true},
            {"kind":"Checks","visible":true},{"kind":"Workflows","visible":true}
        ]}})
    } else {
        json!({"unrecognized_version":999,"cached_metrics":{"private":"never display"}})
    };
    {
        let cache = Cache::open(&database(&fixture)).unwrap();
        cache.save_view(&scope, PROFILE_KEY, &payload).unwrap();
        cache
            .put_draft(&scope, "unrelated-failure", "original pending draft")
            .unwrap();
    }
    let mut steps = refresh(&[APP, TOOLS]);
    steps.extend(gh_fixture::account(42));
    steps.extend(gh_fixture::account(42)); // SQL refusal, no repository query.
    steps.extend(refresh(&[APP, TOOLS])); // Only explicit refresh recovers.
    assert_eq!(steps.len(), 24);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 140, 42);
    start(&mut session, mouse);
    cards_ready(&mut session, 3, 1);
    choose(&mut session, mouse, "[O Open saved]", b"O");
    session.wait_for_screen("Dashboard values unavailable");
    session.wait_for_screen(if outside {
        "Saved scope outside"
    } else {
        "Saved dashboard is invalid"
    });
    for old in [
        "Loaded open PRs",
        "Needs my review",
        "@alice",
        "UTC window [",
        "never display",
    ] {
        absent(&mut session, old);
    }
    journal(&mut session, &fixture, 14);
    assert_eq!(saved(&fixture), payload);
    session.resize(80, 24);
    session.wait_for_screen("Dashboard values unavailable");
    session.focus_footer_control("[r Refresh]");
    if mouse {
        session.click_visible_action("[r Refresh]");
    } else {
        session.send_key(b"\r");
    }
    cards_ready(&mut session, 3, 1);
    journal(&mut session, &fixture, 24);
    exit(&mut session, mouse);
    assert_eq!(saved(&fixture), payload);
    assert_eq!(
        Cache::open(&database(&fixture))
            .unwrap()
            .draft(&scope, "unrelated-failure")
            .unwrap()
            .as_deref(),
        Some("original pending draft")
    );
    preserve(&fixture, &head, &index, &status);
}

#[test]
fn dashboards_keyboard_save_final_new_account_cannot_overwrite_original_profile() {
    fenced_save(false, false);
}
#[test]
fn dashboards_mouse_save_final_new_account_cannot_overwrite_original_profile() {
    fenced_save(true, false);
}
#[test]
fn dashboards_keyboard_cancel_live_save_reaps_child_and_preserves_profile() {
    fenced_save(false, true);
}
#[test]
fn dashboards_mouse_cancel_live_save_reaps_child_and_preserves_profile() {
    fenced_save(true, true);
}
fn fenced_save(mouse: bool, cancelled: bool) {
    let fixture = configured(false);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let scope = Scope::github(gh_fixture::HOST, Some(42)).unwrap();
    let original = json!({"original-profile":"must not be overwritten"});
    {
        let cache = Cache::open(&database(&fixture)).unwrap();
        cache.save_view(&scope, PROFILE_KEY, &original).unwrap();
        cache
            .put_draft(&scope, "guarded-save", "keep original body")
            .unwrap();
    }
    let mut steps = refresh(&[APP]);
    if cancelled {
        steps.extend(gh_fixture::account(42));
        let mut slow = get(&format!("repos/{APP}"), repository(APP));
        slow.delay_ms = 5000;
        steps.push(slow);
    } else {
        let mut save = window(&[APP]);
        save.truncate(save.len() - 2);
        save.extend(gh_fixture::account(43));
        steps.extend(save);
    }
    let count = if cancelled { 10 } else { 13 };
    assert_eq!(steps.len(), count);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    cards_ready(&mut session, 1, 1);
    choose(&mut session, mouse, "[S Save]", b"S");
    session.wait_for_command_count(&fixture, count);
    if cancelled {
        session.wait_for_control("[Esc Cancel]");
        let rows = session
            .inspect_command_journal(&fixture)
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        let pid = i32::try_from(rows.last().unwrap()["pid"].as_u64().unwrap()).unwrap();
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            0,
            "Controlled saved-profile read really is live before Cancel"
        );
        // Disabled while the save runs: Layout leaves the key bar, its key explains why.
        session.assert_control_absent("[L Layout]");
        session.press_disabled_key(b"L", "Action unavailable");
        let close =
            session.wait_for_owned_completed_action("Action unavailable", "[Esc Close]", &[]);
        absent(&mut session, "Loaded open PRs");
        journal(&mut session, &fixture, count);
        if mouse {
            session.click(close.0, close.1);
        } else {
            session.send_key(b"\x1b");
        }
        session.wait_for_overlay_close("Action unavailable");
        session.wait_for_cursorless_condition(
            |frame| {
                let text = frame.contents();
                crate::support::locate_control(frame, "[Esc Cancel]").is_some()
                    && !text.contains("Action unavailable")
                    && !text.contains("Loaded open PRs")
            },
            "disabled explanation closed before explicit live-save cancellation",
            Duration::from_secs(8),
        );
        choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
        session.wait_for_screen("Dashboard read cancelled");
        session.try_wait_for_condition(|_| unsafe {libc::kill(pid,0)} == -1 && std::io::Error::last_os_error().raw_os_error()==Some(libc::ESRCH),"actual cancelled save child reaped",Duration::from_secs(8)).unwrap();
    } else {
        session.wait_for_screen("Dashboard account changed");
    }
    let hidden_label = if cancelled {
        "Dashboard read cancelled"
    } else {
        "Dashboard values unavailable"
    };
    session.wait_for_screen(hidden_label);
    for old in [
        "Loaded open PRs",
        "Needs my review",
        "UTC window [",
        "@alice",
    ] {
        absent(&mut session, old);
    }
    session.resize(80, 24);
    session.wait_for_screen(hidden_label);
    exit(&mut session, mouse);
    journal(&mut session, &fixture, count);
    assert_eq!(saved(&fixture), original);
    let cache = Cache::open(&database(&fixture)).unwrap();
    assert_eq!(
        cache.draft(&scope, "guarded-save").unwrap().as_deref(),
        Some("keep original body")
    );
    assert!(cache
        .saved_views(&Scope::github(gh_fixture::HOST, Some(43)).unwrap())
        .unwrap()
        .is_empty());
    preserve(&fixture, &head, &index, &status);
}
