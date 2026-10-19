//! Registered Dashboard principal, publication-race and cancellation journeys.
//! TASK-61 records executed gates; registration is not itself verification.
use super::*;
use ghtui::{
    pr_write_journal::{PrWriteIntent, PrWriteJournal, PrWriteState, PrWriteTarget},
    providers::gh::{HostIdentity, PrSummary},
};
use rusqlite::{params, Connection};

fn database(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("config/ghtui/cache.sqlite3")
}
fn fixture_identity() -> HostIdentity {
    HostIdentity {
        host: gh_fixture::HOST.into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "2.101.0".into(),
    }
}
fn original_target() -> PrWriteTarget {
    // Canonical fields come from the same exact fixture document used by gh;
    // this creates only a Pending diagnostic, never a provider receipt/effect.
    let raw = alpha();
    let summary:PrSummary=serde_json::from_value(json!({
        "pr":{"host":gh_fixture::HOST,"id":raw["node_id"],"repository":raw["base"]["repo"]["full_name"],"number":raw["number"],"title":raw["title"],"body":raw["body"],"url":raw["html_url"],"author":{"login":raw["user"]["login"],"actor_type":raw["user"]["type"],"display_name":null,"avatar_url":null},"state":raw["state"],"draft":raw["draft"],"created_at":raw["created_at"],"updated_at":raw["updated_at"],"queues":[]},
        "head_sha":raw["head"]["sha"],"base_sha":raw["base"]["sha"],"head_ref":raw["head"]["ref"],"base_ref":raw["base"]["ref"],"merged":raw["merged"],"auto_merge":false,"merge_commit_sha":null,"mergeable":raw["mergeable"],"mergeable_state":raw["mergeable_state"],"additions":raw["additions"],"deletions":raw["deletions"],"changed_files":raw["changed_files"]
    })).unwrap();
    PrWriteTarget::new(&fixture_identity(), &summary).unwrap()
}
fn preserve_snapshot(fixture: &Fixture) -> (String, Vec<u8>, String) {
    (
        fixture.git(&["rev-parse", "HEAD"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fixture.git(&["status", "--porcelain=v2", "-z"]),
    )
}
fn hidden(session: &mut Session, status: &str) {
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains(status)
                    && !screen.contains(TITLE)
                    && !screen.contains(BODY)
                    // The contributor ACTOR cell shows the login without `@`.
                    && !screen.contains("alice")
                    // No Cards table row (the name after the bar and glyph slot).
                    && !screen.contains(" Loaded open PRs  ")
                    && !screen.contains(" Needs my review  ")
                    && !screen.contains("example/app#1")
                    && !screen.contains(APP)
                    && !screen.contains("UTC window [")
                    && !screen.contains("Cohort: selected-repository")
            },
            "complete hidden Dashboard frame with no prior actor/count/target",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
}
fn stale_input_is_not_open(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    old: (u16, u16),
    count: usize,
) {
    let before = session.inspect_command_journal(fixture);
    if mouse {
        session.click(old.0.min(70), old.1.min(18));
    } else {
        session.send_key(b"\r");
    }
    // Keyboard focus readiness proves this input was processed; final post-exit
    // journal below independently catches any late duplicate/background request.
    session.focus_footer_control("[q Quit]");
    journal(session, fixture, count);
    assert_eq!(session.inspect_command_journal(fixture), before);
    assert!(!session.screen().contains(BODY));
    assert!(!session.screen().contains(TITLE));
}
#[derive(Debug, PartialEq, Eq)]
struct PendingRow {
    attempt_id: i64,
    host: String,
    principal: String,
    pr_node: String,
    repository: String,
    number: u64,
    state: String,
    confirmed: Option<String>,
    command: String,
    observed_head: String,
}
fn pending_rows(fixture: &Fixture) -> Vec<PendingRow> {
    let sql = Connection::open(database(fixture)).unwrap();
    let scope = Scope::github(gh_fixture::HOST, Some(42)).unwrap();
    let count: usize = sql
        .query_row(
            "SELECT COUNT(*) FROM pr_write_attempts WHERE host=?1 AND principal=?2",
            params![scope.host, scope.login],
            |row| row.get(0),
        )
        .unwrap();
    assert!(count <= 2, "Unexpected original-account publication growth");
    let rows = sql.prepare("SELECT attempt_id,host,principal,pr_node,repository,pr_number,state,confirmed_json,command,observed_head FROM pr_write_attempts WHERE host=?1 AND principal=?2 ORDER BY attempt_id LIMIT 2").unwrap()
        .query_map(params![scope.host, scope.login],|row|Ok(PendingRow {attempt_id:row.get(0)?,host:row.get(1)?,principal:row.get(2)?,pr_node:row.get(3)?,repository:row.get(4)?,number:row.get(5)?,state:row.get(6)?,confirmed:row.get(7)?,command:row.get(8)?,observed_head:row.get(9)?})).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    assert_eq!(
        rows.len(),
        count,
        "Bounded metadata cannot hide extra attempts"
    );
    rows
}
fn live_fixture_child(record: &Value, boundary: &str) -> i32 {
    assert_eq!(record["matched"], true);
    let pid = i32::try_from(record["pid"].as_u64().unwrap()).unwrap();
    assert!(pid > 0);
    assert_eq!(
        unsafe { libc::kill(pid, 0) },
        0,
        "The deliberately delayed gh child must still be live {boundary}"
    );
    pid
}
fn assert_new_principal_empty(fixture: &Fixture) {
    let scope = Scope::github(gh_fixture::HOST, Some(43)).unwrap();
    let cache = Cache::open(&database(fixture)).unwrap();
    assert!(cache.list(&scope, None).unwrap().is_empty());
    assert!(cache.saved_views(&scope).unwrap().is_empty());
    let sql = Connection::open(database(fixture)).unwrap();
    for table in [
        "records",
        "drafts",
        "saved_views",
        "sync_reservations",
        "checkpoints",
    ] {
        let count: u64 = sql
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE host=?1 AND login=?2"),
                params![scope.host, scope.login],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "Reused login must not carry {table} into UID43");
    }
    for table in ["pr_write_attempts", "pr_write_scopes"] {
        let count: u64 = sql
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE host=?1 AND principal=?2"),
                params![scope.host, scope.login],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "UID43 cannot inherit original publication metadata"
        );
    }
}

#[test]
fn metrics_boundary_keyboard_final_reused_login_new_uid_hides_previous_count_actor_and_open() {
    account_switch(false);
}
#[test]
fn metrics_boundary_mouse_final_reused_login_new_uid_hides_previous_count_actor_and_open() {
    account_switch(true);
}
fn account_switch(mouse: bool) {
    let fixture = configured(false);
    let (head, index, status) = preserve_snapshot(&fixture);
    let private = json!({"query":"uid42-private-source","repository":APP});
    let original_scope = Scope::github(gh_fixture::HOST, Some(42)).unwrap();
    Cache::open(&database(&fixture))
        .unwrap()
        .save_view(&original_scope, "metrics-boundary-private", &private)
        .unwrap();
    let mut steps = refresh(&[APP]);
    let mut changing = window(&[APP]);
    let final_start = changing.len() - 2;
    changing.truncate(final_start);
    changing.extend(gh_fixture::account(43));
    steps.extend(changing);
    assert_eq!(steps.len(), 13);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    journal(&mut session, &fixture, 7);
    cards_ready(&mut session, 1, 1);
    card(&mut session, mouse, 0);
    session.wait_for_screen(TITLE);
    session.wait_for_row(&[TITLE, "alice"]);
    let old = session.locate(TITLE).unwrap();
    choose(&mut session, mouse, "[0 30 days]", b"0");
    journal(&mut session, &fixture, 13);
    hidden(
        &mut session,
        "Dashboard account changed • previous values hidden",
    );
    session.resize(80, 24);
    hidden(
        &mut session,
        "Dashboard account changed • previous values hidden",
    );
    stale_input_is_not_open(&mut session, &fixture, mouse, old, 13);
    assert_new_principal_empty(&fixture);
    metadata_only(&fixture);
    assert_eq!(
        Cache::open(&database(&fixture))
            .unwrap()
            .saved_view(&original_scope, "metrics-boundary-private")
            .unwrap(),
        Some(private)
    );
    preserve(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    journal(&mut session, &fixture, 13);
    assert!(pending_rows(&fixture).is_empty());
}

#[test]
fn metrics_boundary_keyboard_actual_pending_during_final_identity_fences_original_window_without_resend(
) {
    journal_race(false);
}
#[test]
fn metrics_boundary_mouse_actual_pending_during_final_identity_fences_original_window_without_resend(
) {
    journal_race(true);
}
fn journal_race(mouse: bool) {
    let fixture = configured(false);
    let (head, index, status) = preserve_snapshot(&fixture);
    let mut steps = refresh(&[APP]);
    let mut changing = window(&[APP]);
    changing.last_mut().unwrap().delay_ms = 3500;
    steps.extend(changing);
    assert_eq!(steps.len(), 13);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    journal(&mut session, &fixture, 7);
    cards_ready(&mut session, 1, 1);
    card(&mut session, mouse, 0);
    session.wait_for_screen(TITLE);
    let old = session.locate(TITLE).unwrap();
    choose(&mut session, mouse, "[0 30 days]", b"0");
    session.wait_for_screen("prior values hidden");
    session.wait_for_command_count(&fixture, 13);
    let records = session
        .inspect_command_journal(&fixture)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.last().unwrap()["args"][1], "user");
    let last_identity = records.last().unwrap();
    live_fixture_child(last_identity, "before the actual durable Pending begin");
    let target = original_target();
    let mut journal_api = PrWriteJournal::open(&database(&fixture)).unwrap();
    let intent = PrWriteIntent {
        command: "new-comment".into(),
        selected_resource: "pr".into(),
        observed_head: HEAD.into(),
        values: BTreeMap::from([(
            "body".into(),
            "Prepared only: no invocation or remote effect".into(),
        )]),
    };
    let permit = journal_api.begin(&target, &intent).unwrap();
    let id = permit.marker().attempt_id;
    assert_eq!(permit.marker().state, PrWriteState::Pending);
    assert!(permit.marker().confirmed.is_none());
    drop(permit);
    drop(journal_api);
    live_fixture_child(last_identity, "after the actual durable Pending begin");
    hidden(
        &mut session,
        "Dashboard source changed • previous values hidden",
    );
    session.wait_for_screen("Original PR publication is unresolved");
    session.resize(80, 24);
    hidden(
        &mut session,
        "Dashboard source changed • previous values hidden",
    );
    stale_input_is_not_open(&mut session, &fixture, mouse, old, 13);
    journal(&mut session, &fixture, 13);
    let pending = pending_rows(&fixture);
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0],
        PendingRow {
            attempt_id: id,
            host: gh_fixture::HOST.into(),
            principal: "user-id:42".into(),
            pr_node: "PR_dashboard_1".into(),
            repository: APP.into(),
            number: 1,
            state: "pending".into(),
            confirmed: None,
            command: "new-comment".into(),
            observed_head: HEAD.into()
        }
    );
    let generation: u64 = Connection::open(database(&fixture))
        .unwrap()
        .query_row(
            "SELECT generation FROM pr_write_scopes WHERE host=?1 AND principal=?2 AND pr_node=?3",
            params![gh_fixture::HOST, "user-id:42", "PR_dashboard_1"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        generation, 1,
        "Actual begin advanced the previously absent original parent epoch exactly once"
    );
    let restored = PrWriteJournal::open(&database(&fixture))
        .unwrap()
        .blocking(&target)
        .unwrap()
        .unwrap();
    assert_eq!(restored.state, PrWriteState::Pending);
    assert!(restored.confirmed.is_none());
    assert_eq!(restored.payload_sha256, intent.digest().unwrap());
    metadata_only(&fixture);
    preserve(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    journal(&mut session, &fixture, 13);
    assert_eq!(pending_rows(&fixture), pending);
}

#[test]
fn metrics_boundary_keyboard_escape_cancel_then_footer_refresh_retains_one_repo_successful_seven_day_intent(
) {
    cancel_refresh(false);
}
#[test]
fn metrics_boundary_mouse_escape_cancel_then_footer_refresh_retains_one_repo_successful_seven_day_intent(
) {
    cancel_refresh(true);
}
fn cancel_refresh(mouse: bool) {
    let fixture = configured(true);
    let (head, index, status) = preserve_snapshot(&fixture);
    let mut steps = refresh(&[APP, TOOLS]);
    steps.extend(refresh(&[APP]));
    let mut interrupted = gh_fixture::account(42);
    let mut slow = get(&format!("repos/{APP}"), repository(APP));
    slow.delay_ms = 5000;
    interrupted.push(slow);
    assert_eq!(interrupted.len(), 3);
    steps.extend(interrupted);
    steps.extend(refresh(&[APP]));
    assert_eq!(steps.len(), 27);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    journal(&mut session, &fixture, 10);
    cards_ready(&mut session, 3, 1);
    choose(&mut session, mouse, "[R Repositories]", b"R");
    session.wait_for_screen("Choose ALL declared repositories or exactly one");
    if mouse {
        session.click_visible_action(&format!("[{APP}]"));
    } else {
        session.send_key(b"\x1b[B");
        selected(&mut session, &format!("[{APP}]"));
        session.send_key(b"\r");
    }
    journal(&mut session, &fixture, 17);
    cards_ready(&mut session, 1, 1);
    session.wait_for_screen(&format!("Repository: {APP}"));
    let original_clock = as_of(&mut session);
    assert_window(&mut session, 7, original_clock);
    choose(&mut session, mouse, "[0 30 days]", b"0");
    session.wait_for_command_count(&fixture, 20);
    session.wait_for_screen("prior values hidden");
    session.wait_for_control("[Esc Cancel]");
    let requests = session
        .inspect_command_journal(&fixture)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let cancelled = requests.last().unwrap();
    assert_eq!(cancelled["args"][1], format!("repos/{APP}"));
    assert_eq!(cancelled["matched"], true);
    let pid = live_fixture_child(cancelled, "before Escape/SGR cancellation");
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    hidden(
        &mut session,
        "Dashboard read cancelled • previous values hidden",
    );
    session
        .try_wait_for_condition(
            |_| {
                let result = unsafe { libc::kill(pid, 0) };
                result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            },
            "cancelled strict gh child was reaped",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    session.resize(80, 24);
    hidden(
        &mut session,
        "Dashboard read cancelled • previous values hidden",
    );
    journal(&mut session, &fixture, 20);
    let after_cancel = session.inspect_command_journal(&fixture);
    // Tab-focusing the key bar proves the resize and cancellation were processed.
    session.focus_footer_control("[r Refresh]");
    assert_eq!(
        session.inspect_command_journal(&fixture),
        after_cancel,
        "Cancellation cannot launch final principal or another pinned repo read"
    );
    if mouse {
        session.click_visible_action("[r Refresh]");
    } else {
        session.send_key(b"\r");
    }
    journal(&mut session, &fixture, 27);
    cards_ready(&mut session, 1, 1);
    session.wait_for_screen(&format!("Repository: {APP}"));
    let recovered_clock = as_of(&mut session);
    assert_window(&mut session, 7, recovered_clock);
    let all = session
        .inspect_command_journal(&fixture)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let recovery = &all[20..];
    assert_eq!(recovery.len(), 7);
    assert!(recovery.iter().all(|record| record["args"][1]
        .as_str()
        .is_none_or(|endpoint| !endpoint.contains(TOOLS))));
    assert_eq!(
        recovery
            .iter()
            .filter(|record| record["args"][1]
                .as_str()
                .is_some_and(|endpoint| endpoint.contains("pulls?state=all")))
            .count(),
        1
    );
    metadata_only(&fixture);
    assert!(pending_rows(&fixture).is_empty());
    preserve(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    journal(&mut session, &fixture, 27);
}
