//! SOURCE-ONLY child of repository_catalogue_native_v1: UNREGISTERED/UNRUN.
//! Strict schema and scoped runtime/Main admission are prerequisites. Counts
//! below are explicit protocol predictions, never measured passing results.
use super::*;
use ghtui::{
    github_workspace::GithubWorkspace,
    providers::gh::{HostIdentity, PullRequest, Queue},
    storage::{Record, SyncFrame},
};
use std::process::Command;

#[path = "repository_catalogue_measured_refresh_v1.rs"]
mod measured_refresh_v1;

const LOCAL_QUERY: &str = "dirty checkout";
const OLD_CACHE: &str = "PRIVATE_OLD_NAMED_INBOX_CANARY";
const UNADMITTED: &str = "PRIVATE_LATE_OR_MOVING_INBOX_CANARY";
const NEW_FIRST: &str = "example/refreshed0000";
const NEW_SECOND: &str = "example/refreshed0001";

fn fixture(steps: &[GhStep]) -> Fixture {
    let fixture = configured(steps);
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(&fixture)).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database(&fixture)).unwrap());
    fixture
}
fn ids(count: usize) -> Vec<u64> {
    (0..count as u64).map(|i| 10000 + i).collect()
}
fn inventory(count: usize) -> Vec<GhStep> {
    collection(
        vec![json!((0..count as u64).map(repository).collect::<Vec<_>>())],
        42,
    )
}
fn current(index: u64, name: &str) -> Value {
    let mut value = repository(index);
    value["full_name"] = json!(name);
    value
}
fn activation(count: usize) -> Vec<GhStep> {
    let mut steps = principal(42);
    for index in 0..count as u64 {
        steps.push(get(
            &format!("repos/example/repo{index:04}"),
            repository(index),
        ));
    }
    steps.extend(principal(42));
    assert_eq!(steps.len(), count + 4);
    steps
}
fn issue(index: usize, name: &str, title: &str) -> Value {
    // Search response identity is retained, NOT promoted into canonical PR or
    // repository database/node authority. Actual original repo proofs are separate.
    json!({"id":30000+index,"node_id":format!("SEARCH_catalogue_{index}_7"),"number":7,
      "title":title,"body":"Fresh scoped discussion fragment","state":"open","draft":false,
      "repository_url":format!("https://{HOST}/api/v3/repos/{name}"),
      "html_url":format!("https://{HOST}/{name}/pull/7"),
      "pull_request":{"url":format!("https://{HOST}/api/v3/repos/{name}/pulls/7"),"html_url":format!("https://{HOST}/{name}/pull/7")},
      "created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-14T00:00:00Z",
      "user":{"id":42,"node_id":"U_requester_42","login":"octocat","type":"User"}})
}
fn query(index: usize, name: &str, qualifier: &str, title: &str) -> GhStep {
    GhStep::read(
        &[
            "api",
            "search/issues",
            "--hostname",
            HOST,
            "--method",
            "GET",
            "-f",
            &format!("q=is:pr is:open {qualifier}:octocat repo:{name}"),
            "-f",
            "per_page=100",
            "-f",
            "page=1",
        ],
        &json!({"total_count":1,"incomplete_results":false,"items":[issue(index,name,title)]})
            .to_string(),
    )
}
fn refresh_protocol(count: usize, canonical: &[&str], title: &str) -> Vec<GhStep> {
    assert_eq!(count, canonical.len());
    let mut steps = principal(42);
    for (index, name) in canonical.iter().enumerate() {
        steps.push(get(
            &format!("repos/example/repo{index:04}"),
            current(index as u64, name),
        ));
    }
    steps.extend(principal(42));
    for qualifier in ["author", "assignee", "review-requested"] {
        for (index, name) in canonical.iter().enumerate() {
            steps.push(query(index, name, qualifier, title));
        }
    }
    for (index, name) in canonical.iter().enumerate() {
        steps.push(get(
            &format!("repos/example/repo{index:04}"),
            current(index as u64, name),
        ));
    }
    steps.extend(principal(42));
    assert_eq!(steps.len(), 6 + 5 * count);
    steps
}
fn setup_protocol(count: usize) -> Vec<GhStep> {
    let mut steps = inventory(count);
    steps.extend(save_intent());
    steps.extend(activation(count));
    assert_eq!(steps.len(), 15 + count);
    steps
}
fn exact(session: &Session, fixture: &Fixture, steps: &[GhStep]) {
    assert_commands(session, fixture, steps.len());
    let rows = commands(session, fixture);
    for (ordinal, (actual, step)) in rows.iter().zip(steps).enumerate() {
        assert_eq!(actual["step"], ordinal);
        assert_eq!(actual["args"], json!(step.args));
        assert_eq!(actual["stdin_hex"], "");
        assert_eq!(actual["matched"], true);
    }
}
fn row_focus(session: &mut Session, mouse: bool) {
    if !mouse && session.screen().contains("Filter focused:") {
        session.send_key(b"\t");
        ready(session, &[TITLE, "Filter:"]);
    }
}
fn activate_saved(session: &mut Session, mouse: bool) {
    row_focus(session, mouse);
    choose(session, mouse, "[a Activate set]", b"a");
    overlay_ready(session, ACTIVATE, &[SET, "[Enter Activate]"]);
    choose(session, mouse, "[Enter Activate]", b"\r");
    session.wait_for_overlay_close(ACTIVATE);
    ready(session, &[TITLE, "Active explicit set", SET]);
}
fn local_search(session: &mut Session, mouse: bool) {
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
    session.paste(LOCAL_QUERY);
    choose(session, mouse, "[Enter Search]", b"\r");
    local_search_ready(session);
}
fn local_search_ready(session: &mut Session) {
    ready(
        session,
        &[
            "Search • focused view",
            LOCAL_QUERY,
            "1 results • Exact",
            "dirty界.txt",
        ],
    );
    assert!(!session.screen().contains(OLD_CACHE));
    assert!(!session.screen().contains(UNADMITTED));
}
fn open_saved(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize) -> Value {
    local_search(session, mouse);
    assert_commands(session, fixture, 0);
    open_catalogue(session, fixture, mouse, 0);
    discover(session, mouse);
    session.wait_for_command_count(fixture, 7);
    ready(
        session,
        &[
            TITLE,
            &format!("Observed: {count}"),
            "API-visible membership complete",
        ],
    );
    for index in 0..count {
        let name = format!("example/repo{index:04}");
        let label = format!("Repository ID: {}", 10000 + index);
        filter(session, mouse, &name, &name, &label);
        choose(session, mouse, "[Space Select]", b" ");
        ready(session, &[&name, &label, "Selected: yes"]);
    }
    save(session, mouse);
    session.wait_for_command_count(fixture, 11);
    let original = assert_intent(fixture, &ids(count));
    activate_saved(session, mouse);
    session.wait_for_command_count(fixture, 15 + count);
    // The status is bounded to one line beside real Close/Quit controls;
    // original-ID closure is independently proved by the exact journal below.
    ready(session, &[TITLE, "Active explicit set", SET]);
    choose(session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    local_search_ready(session); // Actual original working query/selection survived.
    choose(session, mouse, "1 Inbox", b"1");
    session
        .try_wait_for_condition(
            |screen| {
                let text = screen
                    .lines()
                    .map(|line| line.trim().trim_matches('│').trim())
                    .collect::<Vec<_>>()
                    .join(" ");
                screen.contains("Inbox • focused view")
                    && text.contains("current exact access required")
            },
            "Actual wrapped scoped Inbox explanation, no default query",
            Duration::from_secs(8),
        )
        .unwrap();
    assert_commands(session, fixture, 15 + count);
    original
}
fn source(canonical: &[&str]) -> String {
    let mut canonical = canonical.to_vec();
    canonical.sort_unstable();
    format!(
        "personal-inbox:{}",
        json!({"repositories":canonical,"excluded":[]})
    )
}
fn frame(fixture: &Fixture, canonical: &[&str]) -> Option<SyncFrame> {
    Cache::open(&database(fixture))
        .unwrap()
        .sync_frame(&Scope::github(HOST, Some(42)).unwrap(), &source(canonical))
        .unwrap()
}
fn no_attempts(fixture: &Fixture) {
    let sql = rusqlite::Connection::open(database(fixture)).unwrap();
    for table in ["pr_write_attempts", "actions_write_attempts"] {
        let count: i64 = sql
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            count, 0,
            "Read-only scoped refresh cannot create {table} publication intent"
        );
    }
}
fn seed_old_named(fixture: &Fixture, canonical: &[&str]) {
    let identity = HostIdentity {
        host: HOST.into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "old, not authority".into(),
    };
    let workspace = GithubWorkspace::empty(
        identity,
        canonical.iter().map(|name| (*name).into()).collect(),
        Default::default(),
    );
    let old = PullRequest {
        host: HOST.into(),
        id: "OLD_PRIVATE_SEARCH_NODE".into(),
        repository: canonical[0].into(),
        number: 99,
        title: OLD_CACHE.into(),
        body: OLD_CACHE.into(),
        url: String::new(),
        author: Default::default(),
        state: "open".into(),
        draft: false,
        created_at: String::new(),
        updated_at: String::new(),
        queues: [Queue::Authored].into_iter().collect(),
    };
    Cache::open(&database(fixture))
        .unwrap()
        .commit_sync(
            &workspace.scope().unwrap(),
            &workspace.source(),
            8,
            &[
                Record::new(
                    "inbox-meta",
                    workspace.source(),
                    "",
                    json!({"coverage":{"loaded":1,"total_matches":1,"complete":true,"reasons":[]}}),
                ),
                Record::new(
                    "pr",
                    "cached-private#99",
                    canonical[0],
                    serde_json::to_value(old).unwrap(),
                ),
            ],
            true,
        )
        .unwrap();
}
fn fresh(session: &mut Session, count: usize, title: &str) {
    // The 80-column TITLE cell ellipsizes long titles, so match their distinct head.
    let title = title.get(..24).unwrap_or(title);
    ready(
        session,
        &[
            "Inbox • focused view",
            "My PRs refreshed",
            title,
            &format!(
                "{count} unique PRs across {} queue matches • complete",
                count * 3
            ),
        ],
    );
    assert!(!session.screen().contains(OLD_CACHE));
    assert!(!session.screen().contains("all repositories on host"));
    assert!(!session.screen().contains("Loading Inbox"));
}
fn refresh(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[r Refresh]", b"r");
}
fn help_barrier(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[? Help]", b"?");
    overlay_ready(
        session,
        "Help • keyboard and mouse",
        &["Inbox • help for this view", "[Esc Close]"],
    );
    if mouse {
        session.close_overlay_by_mouse("Help • keyboard and mouse");
    } else {
        session.close_overlay_by_key("Help • keyboard and mouse");
    }
}
fn hidden(session: &mut Session, prefix: &str, title: &str) {
    session.try_wait_for_condition(|screen| screen.contains("Inbox • focused view") && screen.contains(prefix)
      && !screen.contains(title) && !screen.contains(UNADMITTED) && !screen.contains(OLD_CACHE)
      && !screen.contains("unique PRs across") && !screen.contains("Loading Inbox"),
      "Completed scoped failure hides old counts, private row/hits and fresh-but-unadmitted fragment",Duration::from_secs(8)).unwrap();
}

#[test]
fn repo_catalog_scoped_refresh_keyboard_one_repo_rename_exact_queues_local_and_originals() {
    happy(false, 1);
}
#[test]
fn repo_catalog_scoped_refresh_mouse_one_repo_rename_exact_queues_local_and_originals() {
    happy(true, 1);
}
#[test]
fn repo_catalog_scoped_refresh_keyboard_two_repos_complete_scope_not_first_only() {
    happy(false, 2);
}
#[test]
fn repo_catalog_scoped_refresh_mouse_two_repos_complete_scope_not_first_only() {
    happy(true, 2);
}
fn happy(mouse: bool, count: usize) {
    let canonical = if count == 1 {
        vec![NEW_FIRST]
    } else {
        vec![NEW_FIRST, NEW_SECOND]
    };
    let mut steps = setup_protocol(count);
    steps.extend(refresh_protocol(
        count,
        &canonical,
        "Fresh scoped ID-bound PR",
    ));
    steps.extend(refresh_protocol(
        count,
        &canonical,
        "Refreshed navigation intent PR",
    ));
    assert_eq!(steps.len(), 27 + 11 * count);
    let fixture = fixture(&steps);
    seed_old_named(&fixture, &canonical);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original = open_saved(&mut session, &fixture, mouse, count);
    refresh(&mut session, mouse);
    session.wait_for_command_count(&fixture, 21 + 6 * count);
    fresh(&mut session, count, "Fresh scoped ID-bound PR");
    for name in &canonical {
        assert!(session.screen().contains(name));
    }
    exact(&session, &fixture, &steps[..21 + 6 * count]);
    let committed = frame(&fixture, &canonical).unwrap();
    assert_eq!(committed.generation, 9);
    assert!(committed.complete);
    let prs = committed
        .records
        .iter()
        .filter(|row| row.kind == "pr")
        .collect::<Vec<_>>();
    assert_eq!(prs.len(), count);
    assert!(prs
        .iter()
        .all(|row| row.payload["title"] == "Fresh scoped ID-bound PR"
            && row.payload["queues"].as_array().unwrap().len() == 3));
    assert_eq!(
        assert_intent(&fixture, &ids(count)),
        original,
        "Closed observed rename cannot rewrite original names/IDs/nodes"
    );
    // Preserve only user navigation intent across a SECOND genuine fresh read,
    // not a body/model clone. Mouse filter click is pure; keyboard additionally
    // selects the second actual row when available without opening it.
    choose(&mut session, mouse, "[m Assigned to me]", b"m");
    ready(&mut session, &["Queue: Assigned to me"]);
    let selected = if count == 2 && !mouse {
        session.send_key(b"\x1b[B");
        canonical[1]
    } else {
        canonical[0]
    };
    ready(&mut session, &[&format!("▌ {selected}#7")]);
    refresh(&mut session, mouse);
    session.wait_for_command_count(&fixture, steps.len());
    fresh(&mut session, count, "Refreshed navigation intent PR");
    ready(&mut session, &["Queue: Assigned to me"]);
    // Re-focus through the already selected filter without changing a valid
    // selected PrKey. A lost second-row intent would now visibly select first.
    choose(&mut session, mouse, "[m Assigned to me]", b"m");
    ready(&mut session, &[&format!("▌ {selected}#7")]);
    let refreshed = frame(&fixture, &canonical).unwrap();
    assert_eq!(refreshed.generation, 10);
    assert!(refreshed
        .records
        .iter()
        .filter(|row| row.kind == "pr")
        .all(|row| row.payload["title"] == "Refreshed navigation intent PR"));
    assert_eq!(assert_intent(&fixture, &ids(count)), original);
    exact(&session, &fixture, &steps);
    let gh = session.inspect_command_journal(&fixture);
    let trace = git_commands(&fixture);
    // The Inbox key-bar `r Refresh` token, whose stale hit must not survive under Help.
    let refresh_point = session.wait_for_control("[r Refresh]");
    choose(&mut session, mouse, "[? Help]", b"?");
    overlay_ready(
        &mut session,
        "Help • keyboard and mouse",
        &["Inbox • help for this view", "[Esc Close]"],
    );
    if mouse {
        session.click(refresh_point.0, refresh_point.1);
    } else {
        session.send_key(b"r");
    }
    overlay_ready(
        &mut session,
        "Help • keyboard and mouse",
        &["Inbox • help for this view"],
    );
    if mouse {
        session.close_overlay_by_mouse("Help • keyboard and mouse");
    } else {
        session.close_overlay_by_key("Help • keyboard and mouse");
    }
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    fresh(&mut session, count, "Refreshed navigation intent PR");
    assert_eq!(
        session.inspect_command_journal(&fixture),
        gh,
        "Help/resize/stale underlying hit cannot refresh or send default queries"
    );
    assert_eq!(git_commands(&fixture), trace);
    choose(&mut session, mouse, "3 Search", b"3");
    local_search_ready(&mut session);
    assert_eq!(session.inspect_command_journal(&fixture), gh);
    no_attempts(&fixture);
    finish(&mut session, &fixture, mouse, steps.len(), &before, &config);
    exact(&session, &fixture, &steps);
}

#[test]
fn repo_catalog_scoped_refresh_keyboard_moved_or_recreated_repo_hides_without_rollback_claim() {
    changed_repository(false);
}
#[test]
fn repo_catalog_scoped_refresh_mouse_moved_or_recreated_repo_hides_without_rollback_claim() {
    changed_repository(true);
}
fn changed_repository(mouse: bool) {
    for recreated in [false, true] {
        let mut steps = setup_protocol(1);
        steps.extend(refresh_protocol(1, &[FIRST], "Original current PR"));
        let mut failed = if recreated {
            let mut reads = principal(42);
            let mut changed = repository(0);
            changed["id"] = json!(20000);
            changed["node_id"] = json!("REPO_RECREATED_20000");
            reads.push(get(&format!("repos/{FIRST}"), changed));
            reads.extend(principal(42));
            reads
        } else {
            let mut reads = refresh_protocol(1, &[FIRST], UNADMITTED);
            reads.truncate(9);
            reads[8] = get(
                &format!("repos/{FIRST}"),
                current(0, "example/moved-during-read"),
            );
            reads
        };
        let failed_count = failed.len();
        steps.append(&mut failed);
        assert_eq!(steps.len(), 27 + if recreated { 5 } else { 9 });
        let fixture = fixture(&steps);
        let before = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&fixture, mouse);
        let original = open_saved(&mut session, &fixture, mouse, 1);
        refresh(&mut session, mouse);
        session.wait_for_command_count(&fixture, 27);
        fresh(&mut session, 1, "Original current PR");
        let old = frame(&fixture, &[FIRST]).unwrap();
        assert_eq!(old.generation, 1);
        refresh(&mut session, mouse);
        session.wait_for_command_count(&fixture, 27 + failed_count);
        hidden(
            &mut session,
            if recreated {
                "Original repository read capability unavailable"
            } else {
                "Canonical repository/access changed"
            },
            "Original current PR",
        );
        exact(&session, &fixture, &steps);
        let after = frame(&fixture, &[FIRST]).unwrap();
        if recreated {
            assert_eq!(
                after, old,
                "Recreated ID refusal sends no inbox query/cache commit"
            );
        } else {
            assert_eq!(
                after.generation, 2,
                "Fresh derivative cache may precede closing proof; refusal does NOT roll it back"
            );
            assert!(after
                .records
                .iter()
                .any(|row| row.kind == "pr" && row.payload["title"] == UNADMITTED));
            assert!(!session.screen().contains(UNADMITTED));
        }
        assert_eq!(assert_intent(&fixture, &[10000]), original);
        help_barrier(&mut session, mouse);
        assert_commands(&session, &fixture, steps.len());
        choose(&mut session, mouse, "3 Search", b"3");
        local_search_ready(&mut session);
        no_attempts(&fixture);
        finish(&mut session, &fixture, mouse, steps.len(), &before, &config);
    }
}

#[test]
fn repo_catalog_scoped_refresh_keyboard_pre_and_final_uid43_fence_without_default_or_rollback() {
    fenced(false);
}
#[test]
fn repo_catalog_scoped_refresh_mouse_pre_and_final_uid43_fence_without_default_or_rollback() {
    fenced(true);
}
fn fenced(mouse: bool) {
    for final_loss in [false, true] {
        let mut steps = setup_protocol(1);
        steps.extend(refresh_protocol(1, &[FIRST], "Original UID42 scoped PR"));
        if final_loss {
            let mut second = refresh_protocol(1, &[FIRST], UNADMITTED);
            second.truncate(9);
            second.extend(principal(43));
            steps.extend(second);
        } else {
            steps.extend(principal(43));
        }
        assert_eq!(steps.len(), if final_loss { 38 } else { 29 });
        let fixture = fixture(&steps);
        let before = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&fixture, mouse);
        let original = open_saved(&mut session, &fixture, mouse, 1);
        refresh(&mut session, mouse);
        session.wait_for_command_count(&fixture, 27);
        fresh(&mut session, 1, "Original UID42 scoped PR");
        let point = session.locate("Original UID42 scoped PR").unwrap();
        let committed = frame(&fixture, &[FIRST]).unwrap();
        refresh(&mut session, mouse);
        session.wait_for_command_count(&fixture, steps.len());
        hidden(
            &mut session,
            "Scoped inbox principal unavailable",
            "Original UID42 scoped PR",
        );
        assert!(!session.screen().contains("@octocat"));
        if mouse {
            session.click(point.0, point.1);
        } else {
            session.send_key(b"\r");
        }
        help_barrier(&mut session, mouse);
        refresh(&mut session, mouse);
        session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
        ready(&mut session, &["Inbox • focused view", "fenced"]);
        assert_commands(&session, &fixture, steps.len());
        let after = frame(&fixture, &[FIRST]).unwrap();
        if final_loss {
            assert_eq!(
                after.generation, 2,
                "Final principal loss does not pretend derivative SQL was rolled back"
            );
            assert!(after
                .records
                .iter()
                .any(|row| row.kind == "pr" && row.payload["title"] == UNADMITTED));
            assert!(!session.screen().contains(UNADMITTED));
        } else {
            assert_eq!(
                after, committed,
                "Pre-principal loss cannot launch repo/query/cache work"
            );
        }
        assert_eq!(assert_intent(&fixture, &[10000]), original);
        choose(&mut session, mouse, "3 Search", b"3");
        local_search_ready(&mut session);
        no_attempts(&fixture);
        finish(&mut session, &fixture, mouse, steps.len(), &before, &config);
        exact(&session, &fixture, &steps);
    }
}

struct StoppedRead {
    pid: i32,
    stopped: bool,
}
impl StoppedRead {
    fn stop(session: &mut Session, fixture: &Fixture, count: usize, expected: &GhStep) -> Self {
        session.wait_for_command_count(fixture, count);
        let rows = commands(session, fixture);
        assert_eq!(rows.len(), count);
        let child = &rows[count - 1];
        assert_eq!(child["matched"], true);
        assert_eq!(child["args"], json!(expected.args));
        let pid = i32::try_from(child["pid"].as_u64().unwrap()).unwrap();
        assert!(pid > 0);
        // SAFETY: only the exact positive live child from this strict fixture's journal.
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        let guard = Self { pid, stopped: true };
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
                "Actual owned scoped-principal child stopped before response",
                Duration::from_secs(8),
            )
            .unwrap();
        guard
    }
    fn reaped(&mut self, session: &mut Session) {
        // Cancellation may already have killed the stopped child; never signal
        // a group or infer successful completion from command-entry count.
        if unsafe { libc::kill(self.pid, 0) } == 0 {
            assert_eq!(unsafe { libc::kill(self.pid, libc::SIGCONT) }, 0);
        }
        self.stopped = false;
        session.try_wait_for_condition(|_| unsafe {libc::kill(self.pid,0)}==-1 && std::io::Error::last_os_error().raw_os_error()==Some(libc::ESRCH),
          "Retired scoped refresh child actually reaped",Duration::from_secs(8)).unwrap();
    }
}
impl Drop for StoppedRead {
    fn drop(&mut self) {
        if self.stopped {
            unsafe { libc::kill(self.pid, libc::SIGCONT) };
        }
    }
}

#[test]
fn repo_catalog_scoped_refresh_keyboard_retired_pending_principal_cannot_hide_new_empty_owner() {
    retired(false);
}
#[test]
fn repo_catalog_scoped_refresh_mouse_retired_pending_principal_cannot_hide_new_empty_owner() {
    retired(true);
}
fn retired(mouse: bool) {
    let mut steps = inventory(1);
    steps.extend(save_intent());
    steps.extend(refresh_protocol(1, &[FIRST], "Original owner admitted PR"));
    let mut stale = principal(43);
    stale[1].delay_ms = 1000;
    steps.extend(stale.clone());
    steps.extend(inventory(1));
    assert_eq!(steps.len(), 31);
    let fixture = fixture(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    local_search(&mut session, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    ready(&mut session, &[TITLE, "Observed: 1", FIRST]);
    // Explicitly save EMPTY metadata intent; Open-one is separately deliberate
    // current target navigation, never automatic activation of that saved set.
    row_focus(&mut session, mouse);
    save(&mut session, mouse);
    let original = assert_intent(&fixture, &[]);
    row_focus(&mut session, mouse);
    choose(&mut session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_command_count(&fixture, 22);
    session.wait_for_overlay_close(TITLE);
    ready(
        &mut session,
        &[
            "Inbox • focused view",
            "Original owner admitted PR",
            "1 unique PRs across 3 queue matches • complete",
        ],
    );
    let committed = frame(&fixture, &[FIRST]).unwrap();
    refresh(&mut session, mouse);
    let mut stopped = StoppedRead::stop(&mut session, &fixture, 24, &stale[1]);
    ready(&mut session, &["Loading Inbox"]);
    open_catalogue(&mut session, &fixture, mouse, 24);
    discover(&mut session, mouse);
    session.wait_for_command_count(&fixture, 31);
    ready(&mut session, &[TITLE, "Observed: 1", FIRST]);
    activate_saved(&mut session, mouse);
    ready(
        &mut session,
        &[TITLE, "Active explicit set", SET, "explicit empty"],
    );
    stopped.reaped(&mut session);
    // A current genuine modal remains, not a private-cleared UID43 or late
    // failure notice. Independent runtime tests exercise exact Arc replacement
    // WITHOUT cancellation; this PTY exercises actual retirement/cancel/reap.
    assert!(!session.screen().contains("private repositories hidden"));
    assert!(!session
        .screen()
        .contains("Scoped inbox principal unavailable"));
    assert!(!session.screen().contains("Original owner admitted PR"));
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    choose(&mut session, mouse, "1 Inbox", b"1");
    ready(
        &mut session,
        &["Inbox • focused view", "Explicit repository scope is empty"],
    );
    refresh(&mut session, mouse);
    help_barrier(&mut session, mouse);
    assert_commands(&session, &fixture, 31);
    assert_eq!(frame(&fixture, &[FIRST]).unwrap(), committed);
    assert_eq!(assert_intent(&fixture, &[]), original);
    choose(&mut session, mouse, "3 Search", b"3");
    local_search_ready(&mut session);
    no_attempts(&fixture);
    finish(&mut session, &fixture, mouse, 31, &before, &config);
    exact(&session, &fixture, &steps);
}

#[test]
fn repo_catalog_scoped_refresh_keyboard_original_sql_cas_refusal_hides_committed_derivative() {
    changed_sql(false);
}
#[test]
fn repo_catalog_scoped_refresh_mouse_original_sql_cas_refusal_hides_committed_derivative() {
    changed_sql(true);
}
fn changed_sql(mouse: bool) {
    let mut steps = setup_protocol(1);
    let mut read = refresh_protocol(1, &[FIRST], UNADMITTED);
    read.truncate(9);
    steps.extend(read);
    assert_eq!(steps.len(), 25);
    let fixture = fixture(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original = open_saved(&mut session, &fixture, mouse, 1);
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let cache = Cache::open(&database(&fixture)).unwrap();
    let (key, _) = cache
        .saved_views(&scope)
        .unwrap()
        .into_iter()
        .find(|(_, value)| value["sets"][SET].is_object())
        .unwrap();
    drop(cache);
    let sql = rusqlite::Connection::open(database(&fixture)).unwrap();
    sql.execute_batch("CREATE TRIGGER scoped_original_cas AFTER INSERT ON checkpoints WHEN NEW.source LIKE 'personal-inbox:%' BEGIN UPDATE saved_views SET payload=json_set(payload,'$.independent_user_edit','PRIVATE_SQL_SCOPE_EDIT_CANARY') WHERE host='github.invalid' AND login='user-id:42'; END;").unwrap();
    drop(sql);
    refresh(&mut session, mouse);
    session.wait_for_command_count(&fixture, 25);
    hidden(
        &mut session,
        "Original repository intent boundary changed",
        UNADMITTED,
    );
    let committed = frame(&fixture, &[FIRST]).unwrap();
    assert_eq!(committed.generation, 1);
    assert!(committed
        .records
        .iter()
        .any(|row| row.kind == "pr" && row.payload["title"] == UNADMITTED));
    let cache = Cache::open(&database(&fixture)).unwrap();
    let saved = cache
        .saved_view_bounded(&scope, &key, 128 * 1024)
        .unwrap()
        .unwrap();
    assert_eq!(saved["sets"], original["sets"]);
    assert_eq!(saved["pins"], original["pins"]);
    assert_eq!(
        saved["independent_user_edit"],
        "PRIVATE_SQL_SCOPE_EDIT_CANARY"
    );
    assert_eq!(
        cache.draft(&scope, "catalogue-draft").unwrap().as_deref(),
        Some("Original42 draft")
    );
    assert!(cache
        .list(&Scope::github(HOST, Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
    help_barrier(&mut session, mouse);
    assert_commands(&session, &fixture, 25);
    choose(&mut session, mouse, "3 Search", b"3");
    local_search_ready(&mut session);
    no_attempts(&fixture);
    finish(&mut session, &fixture, mouse, 25, &before, &config);
    exact(&session, &fixture, &steps);
}
