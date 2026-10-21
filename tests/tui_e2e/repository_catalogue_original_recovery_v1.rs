//! Source-only actual-binary recovery journeys. Sixteen declarations UNRUN;
//! counts below are strict planned argv protocols, not passing measurements.
use super::*;
use rusqlite::{params, Connection};
use std::process::Command;

const UNAVAILABLE: &str = "Repository re-read unavailable";
const PREVIEW: &str = "Verify original repository source";
const WARNING: &str = "Unsaved repository choices";
const SECOND: &str = "example/repo0001";

fn inventory(rows: Vec<Value>, uid: u64) -> Vec<GhStep> {
    collection(vec![json!(rows)], uid)
}
fn unavailable() -> Vec<GhStep> {
    let mut steps = inventory(vec![], 42);
    steps[3].status = 1;
    steps[3].stdout.clear();
    steps[3].stderr = b"HTTP 403 PRIVATE_RECOVERY_STDERR_CANARY".to_vec();
    steps
}
fn baseline() -> Vec<GhStep> {
    let mut steps = inventory((0..3).map(repository).collect(), 42);
    steps.extend(save_intent());
    steps.extend(unavailable());
    assert_eq!(steps.len(), 18, "Source7+Save4+genuine closed403 reread7");
    steps
}
fn generation(fixture: &Fixture) -> u64 {
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let sql = Connection::open(database(fixture)).unwrap();
    let rows = sql.prepare("SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source LIKE 'repository-catalogue-source-v1:%' LIMIT 2")
        .unwrap().query_map(params![scope.host, scope.login], |row| row.get::<_,u64>(0)).unwrap()
        .collect::<Result<Vec<_>,_>>().unwrap();
    assert_eq!(
        rows.len(),
        1,
        "Exact original issuer/source, no unproved UID43 reservation"
    );
    rows[0]
}
fn exact(session: &Session, fixture: &Fixture, steps: &[GhStep]) {
    assert_commands(session, fixture, steps.len());
    for (ordinal, (actual, expected)) in commands(session, fixture).iter().zip(steps).enumerate() {
        assert_eq!(actual["step"], ordinal);
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["stdin_hex"], "");
    }
}
fn current_row(session: &mut Session, mouse: bool, query: &str, name: &str, id: u64) {
    choose(session, mouse, "[/ Filter]", b"/");
    session.send_key(b"\x01");
    session.paste(query);
    ready(
        session,
        &[
            TITLE,
            &format!("Filter focused: {query}"),
            "matches 1",
            name,
            &format!("Repository ID: {id}"),
            "[Space Select]",
            "[p Pin]",
        ],
    );
    if mouse {
        session.click_visible_action(&format!("◆ repository · {name}"));
    } else {
        session.send_key(b"\t");
    }
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains(TITLE)
                    && screen.contains(&format!("Filter: {query}"))
                    && !screen.contains("Filter focused:")
            },
            "Genuine current list focus before action",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn initial(session: &mut Session, fixture: &Fixture, mouse: bool) -> Value {
    let original = live(session, fixture, mouse);
    failed_reload(session, fixture, mouse, &original);
    original
}
fn live(session: &mut Session, fixture: &Fixture, mouse: bool) -> Value {
    open_catalogue(session, fixture, mouse, 0);
    discover(session, mouse);
    ready(
        session,
        &[TITLE, "Observed: 3", "API-visible membership complete"],
    );
    current_row(session, mouse, FIRST, FIRST, 10000);
    choose(session, mouse, "[Space Select]", b" ");
    ready(session, &["Selected: yes"]);
    choose(session, mouse, "[p Pin]", b"p");
    ready(session, &["Pinned: yes"]);
    save(session, mouse);
    session.wait_for_command_count(fixture, 11);
    let original = assert_intent(fixture, &[10000]);
    current_row(session, mouse, SECOND, SECOND, 10001);
    choose(session, mouse, "[Space Select]", b" ");
    ready(session, &["Selected: yes"]);
    choose(session, mouse, "[p Pin]", b"p");
    ready(session, &["Pinned: yes"]);
    original
}
fn failed_reload(session: &mut Session, fixture: &Fixture, mouse: bool, original: &Value) {
    choose(session, mouse, "[r Retry]", b"r");
    session.wait_for_command_count(fixture, 18);
    hidden_slot(session);
    assert_eq!(generation(fixture), 2);
    assert_eq!(&assert_intent(fixture, &[10000]), original);
}
fn hidden_slot(session: &mut Session) {
    overlay_ready(
        session,
        UNAVAILABLE,
        &[
            "Previous rows, query and counts are hidden",
            "[v Verify original source]",
            "[x Discard retained choices]",
        ],
    );
    for private in [
        SECOND,
        "Repository ID: 10001",
        "Observed: 3",
        "PRIVATE_RECOVERY_STDERR_CANARY",
    ] {
        assert!(
            !session.screen().contains(private),
            "Opaque slot must not reveal {private}"
        );
    }
}
fn preview(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[v Verify original source]", b"v");
    overlay_ready(
        session,
        PREVIEW,
        &["Fresh page-1 read", "[Enter Verify]", "[Esc Back]"],
    );
}
fn list_focus(session: &mut Session, mouse: bool) {
    if !mouse && session.screen().contains("Filter focused:") {
        session.send_key(b"\t");
        session
            .try_wait_for_condition(
                |screen| {
                    screen.contains(TITLE)
                        && screen.contains("Filter:")
                        && !screen.contains("Filter focused:")
                },
                "Actual list focus before action shortcut",
                Duration::from_secs(8),
            )
            .unwrap();
    }
}
fn verify(session: &mut Session, mouse: bool) {
    preview(session, mouse);
    choose(session, mouse, "[Enter Verify]", b"\r");
    session.wait_for_overlay_close(PREVIEW);
}
fn close(session: &mut Session, mouse: bool, retained: bool) {
    let close_current = |session: &mut Session| {
        let (x, y) = session.wait_for_owned_completed_action(
            TITLE,
            "[Esc Close]",
            &[WARNING, PREVIEW, "Action unavailable • explanation only"],
        );
        if mouse {
            session.click(x, y);
        } else {
            session.send_key(b"\x1b");
        }
    };
    close_current(session);
    if retained {
        overlay_ready(
            session,
            WARNING,
            &["Back keeps them", "[x Discard & Close]"],
        );
        choose(session, mouse, "[Esc Back]", b"\x1b");
        session.wait_for_overlay_close(WARNING);
        close_current(session);
        overlay_ready(session, WARNING, &["[x Discard & Close]"]);
        choose(session, mouse, "[x Discard & Close]", b"x");
        session.wait_for_overlay_close(WARNING);
    }
    session.wait_for_overlay_close(TITLE);
    session.wait_for_main_frame();
}
fn edits(fixture: &Fixture, original: &Value) -> Value {
    let mut changed = original.clone();
    changed["sets"]["CONCURRENT_RECOVERY_SET"] = original["sets"][SET].clone();
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let sql = Connection::open(database(fixture)).unwrap();
    let names = sql.prepare("SELECT name FROM saved_views WHERE host=?1 AND login=?2 AND name LIKE 'repository-catalogue-intent-v1:%' LIMIT 2").unwrap()
        .query_map(params![scope.host,scope.login], |r| r.get::<_,String>(0)).unwrap()
        .collect::<Result<Vec<_>,_>>().unwrap();
    assert_eq!(names.len(), 1);
    assert_eq!(
        sql.execute(
            "UPDATE saved_views SET payload=?1 WHERE host=?2 AND login=?3 AND name=?4",
            params![changed.to_string(), scope.host, scope.login, names[0]]
        )
        .unwrap(),
        1
    );
    changed
}

#[test]
fn repo_catalog_original_recovery_keyboard_same_uid_rename_preview_back_and_exact_restore() {
    happy(false, false);
}
#[test]
fn repo_catalog_original_recovery_mouse_same_uid_rename_preview_back_and_exact_restore() {
    happy(true, false);
}
fn happy(mouse: bool, local_search: bool) {
    let mut current = (0..3).map(repository).collect::<Vec<_>>();
    current[1]["permissions"]["pull"] = json!(false);
    let mut steps = baseline();
    let mut next = inventory(current, 42);
    next[6] = get("user", json!({"id":42,"login":"renamed-account"}));
    steps.extend(next);
    assert_eq!(steps.len(), 25);
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    if local_search {
        working_search(&mut session, mouse);
    }
    let original = initial(&mut session, &fixture, mouse);
    let journal = session.inspect_command_journal(&fixture);
    let git = git_commands(&fixture);
    preview(&mut session, mouse);
    session.resize_owned_modal(80, 24, PREVIEW, &["[PgUp]", "[PgDn]"]);
    let point = session.locate("[Enter Verify]").unwrap();
    session.wheel(point.0, point.1.saturating_sub(3), true);
    choose(&mut session, mouse, "[PgDn]", b"\x1b[6~");
    choose(&mut session, mouse, "[PgUp]", b"\x1b[5~");
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(PREVIEW);
    hidden_slot(&mut session);
    assert_eq!(session.inspect_command_journal(&fixture), journal);
    assert_eq!(git_commands(&fixture), git);
    assert_eq!(generation(&fixture), 2);
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    verify(&mut session, mouse);
    session.wait_for_command_count(&fixture, 25);
    ready(
        &mut session,
        &[
            TITLE,
            "Fresh page-1 read",
            "re-activation required",
            SECOND,
            "Selected: yes",
            "Pinned: yes",
            "Pull permission: false",
            "@renamed-account",
        ],
    );
    assert_eq!(generation(&fixture), 3);
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    close(&mut session, mouse, true);
    if local_search {
        working_ready(&mut session);
    }
    exact(&session, &fixture, &steps);
    finish(&mut session, &fixture, mouse, 25, &before, &config);
}

#[test]
fn repo_catalog_original_recovery_keyboard_repeated_closed403_has_explicit_next_generation_only() {
    repeated(false);
}
#[test]
fn repo_catalog_original_recovery_mouse_repeated_closed403_has_explicit_next_generation_only() {
    repeated(true);
}
fn repeated(mouse: bool) {
    for raw_limit in [false, true] {
        repeated_case(mouse, raw_limit);
    }
}
fn repeated_case(mouse: bool, raw_limit: bool) {
    let mut steps = baseline();
    let mut next = unavailable();
    if raw_limit {
        // Real combined raw-output failure exceeds the unchanged catalogue
        // page4MiB cap. Closing allowance remains in the SAME16MiB budget.
        next[3].status = 0;
        next[3].stderr = vec![0x07; 4 * 1024 * 1024 + 4096];
    }
    steps.extend(next);
    steps.extend(inventory((0..3).map(repository).collect(), 42));
    assert_eq!(steps.len(), 32);
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original = initial(&mut session, &fixture, mouse);
    verify(&mut session, mouse);
    session.wait_for_command_count(&fixture, 25);
    hidden_slot(&mut session);
    assert_eq!(generation(&fixture), 3);
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    // Idle/disabled input cannot treat closed acceptance as automatic retry.
    session.send_key(b"rwsa");
    hidden_slot(&mut session);
    assert_commands(&session, &fixture, 25);
    verify(&mut session, mouse);
    session.wait_for_command_count(&fixture, 32);
    ready(
        &mut session,
        &[
            TITLE,
            "Fresh page-1 read",
            SECOND,
            "Selected: yes",
            "Pinned: yes",
        ],
    );
    assert_eq!(generation(&fixture), 4);
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    close(&mut session, mouse, true);
    exact(&session, &fixture, &steps);
    finish(&mut session, &fixture, mouse, 32, &before, &config);
}

#[test]
fn repo_catalog_original_recovery_keyboard_missing_changed_node_and_empty_never_save_absence() {
    unresolved(false);
}
#[test]
fn repo_catalog_original_recovery_mouse_missing_changed_node_and_empty_never_save_absence() {
    unresolved(true);
}
#[test]
fn completed_frame_actions_catalogue_keyboard_nested_overlay_close_keeps_unresolved_choices() {
    unresolved(false);
}
#[test]
fn completed_frame_actions_catalogue_mouse_nested_overlay_close_keeps_unresolved_choices() {
    unresolved(true);
}
fn unresolved(mouse: bool) {
    for variant in 0..3 {
        let rows = match variant {
            0 => vec![repository(0)],
            1 => {
                let mut r = repository(1);
                r["node_id"] = json!("CURRENT_DIFFERENT_NODE");
                vec![r]
            }
            _ => vec![],
        };
        let mut steps = baseline();
        steps.extend(inventory(rows, 42));
        let fixture = configured(&steps);
        let before = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&fixture, mouse);
        let original = initial(&mut session, &fixture, mouse);
        verify(&mut session, mouse);
        session.wait_for_command_count(&fixture, 25);
        ready(
            &mut session,
            &[TITLE, "Fresh page-1 read", "[? Unresolved]", "[w Save set]"],
        );
        list_focus(&mut session, mouse);
        choose(&mut session, mouse, "[w Save set]", b"w");
        overlay_ready(
            &mut session,
            "Action unavailable • explanation only",
            &["Unresolved local choices retained", "[Esc Close]"],
        );
        assert_commands(&session, &fixture, 25);
        assert_eq!(assert_intent(&fixture, &[10000]), original);
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        session.wait_for_overlay_close("Action unavailable • explanation only");
        choose(&mut session, mouse, "[? Unresolved]", b"?");
        overlay_ready(
            &mut session,
            "Action unavailable • explanation only",
            &["10001", "[Esc Close]"],
        );
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        session.wait_for_overlay_close("Action unavailable • explanation only");
        assert_eq!(generation(&fixture), 3);
        close(&mut session, mouse, true);
        exact(&session, &fixture, &steps);
        finish(&mut session, &fixture, mouse, 25, &before, &config);
    }
}

#[test]
fn repo_catalog_original_recovery_keyboard_pre_and_final_reused_login_uid43_retire_without_copy() {
    account(false);
}
#[test]
fn repo_catalog_original_recovery_mouse_pre_and_final_reused_login_uid43_retire_without_copy() {
    account(true);
}
fn account(mouse: bool) {
    for final_loss in [false, true] {
        let mut steps = baseline();
        if final_loss {
            steps.extend(inventory(vec![repository(2050)], 43));
        } else {
            steps.extend(principal(43));
        }
        assert_eq!(steps.len(), if final_loss { 25 } else { 20 });
        let fixture = configured(&steps);
        let before = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&fixture, mouse);
        let original = initial(&mut session, &fixture, mouse);
        verify(&mut session, mouse);
        session.wait_for_command_count(&fixture, steps.len());
        ready(
            &mut session,
            &[TITLE, "private repositories hidden", "[Esc Close]"],
        );
        for private in [
            SECOND,
            "example/repo2050",
            "Observed: 3",
            "[v Verify original source]",
            "[x Discard retained choices]",
        ] {
            assert!(
                !session.screen().contains(private),
                "Lost proof must retire, not retain {private}"
            );
        }
        assert_eq!(generation(&fixture), if final_loss { 3 } else { 2 });
        assert_eq!(assert_intent(&fixture, &[10000]), original);
        close(&mut session, mouse, false);
        exact(&session, &fixture, &steps);
        finish(&mut session, &fixture, mouse, steps.len(), &before, &config);
    }
}

#[test]
fn repo_catalog_original_recovery_keyboard_concurrent_saved_cas_before_and_after_reservation_refuses(
) {
    concurrent(false);
}
#[test]
fn repo_catalog_original_recovery_mouse_concurrent_saved_cas_before_and_after_reservation_refuses()
{
    concurrent(true);
}
fn concurrent(mouse: bool) {
    for after_reserve in [false, true] {
        let mut steps = baseline();
        let mut next = inventory(vec![repository(1)], 42);
        if after_reserve {
            next[3].delay_ms = 3000;
            steps.extend(next[..4].iter().cloned());
        } else {
            steps.extend(next[..3].iter().cloned());
        }
        let fixture = configured(&steps);
        let before = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&fixture, mouse);
        let original = initial(&mut session, &fixture, mouse);
        let changed = if after_reserve {
            verify(&mut session, mouse);
            let mut stopped = StoppedChild::stop(&mut session, &fixture, 22);
            assert_eq!(generation(&fixture), 3);
            let changed = edits(&fixture, &original);
            stopped.resume();
            changed
        } else {
            let changed = edits(&fixture, &original);
            verify(&mut session, mouse);
            changed
        };
        session.wait_for_command_count(&fixture, steps.len());
        ready(
            &mut session,
            &[TITLE, "private repositories hidden", "[Esc Close]"],
        );
        assert_eq!(generation(&fixture), if after_reserve { 3 } else { 2 });
        assert_eq!(assert_intent(&fixture, &[10000]), changed);
        assert!(!session.screen().contains("CONCURRENT_RECOVERY_SET"));
        close(&mut session, mouse, false);
        exact(&session, &fixture, &steps);
        finish(&mut session, &fixture, mouse, steps.len(), &before, &config);
    }
}

struct StoppedChild {
    pid: i32,
    stopped: bool,
}
impl StoppedChild {
    fn stop(session: &mut Session, fixture: &Fixture, count: usize) -> Self {
        session.wait_for_command_count(fixture, count);
        let journal = commands(session, fixture);
        let child = &journal[count - 1];
        assert_eq!(child["matched"], true);
        let pid = i32::try_from(child["pid"].as_u64().unwrap()).unwrap();
        assert!(pid > 0);
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        let stopped = Self { pid, stopped: true };
        session
            .try_wait_for_condition(
                |_| {
                    let output = Command::new("ps")
                        .args(["-p", &pid.to_string(), "-o", "stat="])
                        .output()
                        .unwrap();
                    output.status.success()
                        && String::from_utf8_lossy(&output.stdout)
                            .trim_start()
                            .starts_with('T')
                },
                "Actual strict owned recovery child stopped",
                Duration::from_secs(8),
            )
            .unwrap();
        stopped
    }
    fn resume(&mut self) {
        // The child may be reaped between any liveness probe and SIGCONT, so only a
        // failure other than "no such process" is an error.
        if unsafe { libc::kill(self.pid, libc::SIGCONT) } != 0 {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
        }
        self.stopped = false;
    }
    fn reaped(&mut self, session: &mut Session) {
        self.resume();
        session.try_wait_for_condition(|_|unsafe{libc::kill(self.pid,0)}==-1
        &&std::io::Error::last_os_error().raw_os_error()==Some(libc::ESRCH),"Retired recovery child actually reaped",Duration::from_secs(8)).unwrap();
    }
}
impl Drop for StoppedChild {
    fn drop(&mut self) {
        if self.stopped {
            unsafe { libc::kill(self.pid, libc::SIGCONT) };
        }
    }
}

#[test]
fn repo_catalog_original_recovery_keyboard_pending_back_then_discard_close_cancels_old_owner() {
    cancel(false);
}
#[test]
fn repo_catalog_original_recovery_mouse_pending_back_then_discard_close_cancels_old_owner() {
    cancel(true);
}
fn cancel(mouse: bool) {
    let mut steps = baseline();
    let mut pending = inventory(vec![repository(2050)], 42);
    pending[3].delay_ms = 60000;
    steps.extend(pending[..4].iter().cloned());
    steps.extend(inventory(vec![repository(2)], 42));
    assert_eq!(steps.len(), 29);
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original = initial(&mut session, &fixture, mouse);
    verify(&mut session, mouse);
    let mut stopped = StoppedChild::stop(&mut session, &fixture, 22);
    ready(
        &mut session,
        &[TITLE, "Original worker pending", "[Esc Close]"],
    );
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    overlay_ready(
        &mut session,
        WARNING,
        &["[Esc Back]", "[x Discard & Close]"],
    );
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(WARNING);
    ready(&mut session, &[TITLE, "Original worker pending"]);
    assert_commands(&session, &fixture, 22);
    close(&mut session, mouse, true);
    stopped.reaped(&mut session);
    assert_eq!(generation(&fixture), 3);
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    open_catalogue(&mut session, &fixture, mouse, 22);
    discover(&mut session, mouse);
    session.wait_for_command_count(&fixture, 29);
    ready(&mut session, &[TITLE, "Observed: 1", "example/repo0002"]);
    assert!(!session.screen().contains("example/repo2050") && !session.screen().contains(SECOND));
    assert_eq!(generation(&fixture), 4);
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    close(&mut session, mouse, false);
    exact(&session, &fixture, &steps);
    finish(&mut session, &fixture, mouse, 29, &before, &config);
}

#[test]
fn repo_catalog_original_recovery_keyboard_original_deadline_no_final_probe_or_retained_slot() {
    timed(false);
}
#[test]
fn repo_catalog_original_recovery_mouse_original_deadline_no_final_probe_or_retained_slot() {
    timed(true);
}
fn timed(mouse: bool) {
    let mut steps = baseline();
    let mut next = inventory(vec![repository(2050)], 42);
    next[3].delay_ms = 3000;
    steps.extend(next[..4].iter().cloned());
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    // Set the local one-second limit after the initial real read, before the
    // closed reread. Only editing/Apply is zero API/SQL, not the earlier load.
    let original = live(&mut session, &fixture, mouse);
    let journal = session.inspect_command_journal(&fixture);
    let sql_generation = generation(&fixture);
    choose(&mut session, mouse, "[l Limits]", b"\x1b[17~");
    overlay_ready(
        &mut session,
        "Repository discovery limits",
        &["Pages (1..1000)", "[Enter Apply]"],
    );
    for (index, value) in ["20", "2000", "8", "16", "1"].iter().enumerate() {
        if mouse {
            session.click_visible_action(
                [
                    "Pages (1..1000)",
                    "Raw rows (100..100000)",
                    "Metadata MiB (1..32)",
                    "Raw capture MiB (1..64)",
                    "Deadline seconds (1..300)",
                ][index],
            );
        }
        session.send_key(b"\x01");
        session.paste(value);
        if !mouse {
            session.send_key(b"\r");
        }
    }
    if !mouse {
        session.focus_visible_action("[Enter Apply]");
    }
    choose(&mut session, mouse, "[Enter Apply]", b"\r");
    session.wait_for_overlay_close("Repository discovery limits");
    assert_eq!(session.inspect_command_journal(&fixture), journal);
    assert_eq!(generation(&fixture), sql_generation);
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    failed_reload(&mut session, &fixture, mouse, &original);
    verify(&mut session, mouse);
    session.wait_for_command_count(&fixture, 22);
    ready(
        &mut session,
        &[TITLE, "private repositories hidden", "[Esc Close]"],
    );
    assert!(
        !session.screen().contains("example/repo2050")
            && !session.screen().contains("[v Verify original source]")
    );
    assert_eq!(generation(&fixture), 3);
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    close(&mut session, mouse, false);
    exact(&session, &fixture, &steps);
    finish(&mut session, &fixture, mouse, 22, &before, &config);
}

fn working_search(session: &mut Session, mouse: bool) {
    choose(session, mouse, "3 Search", b"3");
    ready(
        session,
        &[
            "Search • focused view",
            "Selected local search corpus ready",
            "[/ Query]",
        ],
    );
    session.search_option(mouse, "e Exact", b"e");
    choose(session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste("dirty checkout");
    choose(session, mouse, "[Enter Search]", b"\r");
    working_ready(session);
}
fn working_ready(session: &mut Session) {
    ready(
        session,
        &[
            "Search • focused view",
            "dirty checkout",
            "1 results • Exact",
            "dirty界.txt",
        ],
    );
}
#[test]
fn repo_catalog_original_recovery_keyboard_working_search_and_git_independent() {
    happy(false, true);
}
#[test]
fn repo_catalog_original_recovery_mouse_working_search_and_git_independent() {
    happy(true, true);
}
