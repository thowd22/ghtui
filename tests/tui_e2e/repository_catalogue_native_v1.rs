//! SOURCE-ONLY native Task9 handoff: six genuine subprocess journeys, UNREGISTERED
//! and UNRUN. Selectors/protocol are Root-approved future ABI, NOT current product
//! functionality. No direct App handlers, fake source receipts or live mutations.
use crate::support::{Fixture, GhStep, Session};
use ghtui::{
    config::Config,
    storage::{Cache, Scope},
};
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

const HOST: &str = "github.invalid";
const TITLE: &str = "Repositories • explicit discovery";
const SOURCE: &str = "Repository discovery source";
const SAVE: &str = "Save repository set";
const ACTIVATE: &str = "Activate repository set";
const SET: &str = "catalog-focus";
const FIRST: &str = "example/repo0000";
const LAST: &str = "example/repo1000";

#[path = "repository_catalogue_local_mappings_v1.rs"]
mod local_mappings_v1;
#[path = "repository_catalogue_read_limits_v1.rs"]
mod read_limits_v1;

#[path = "repository_catalogue_source_transition_v1.rs"]
mod source_transition_v1;

#[path = "repository_catalogue_original_recovery_v1.rs"]
mod original_recovery_v1;

#[path = "repository_catalogue_actions_v1.rs"]
mod actions_v1;
#[path = "repository_catalogue_auto_refresh_v1.rs"]
mod auto_refresh_v1;
#[path = "repository_catalogue_avatars_v1.rs"]
mod avatars_v1;
#[path = "repository_catalogue_inbox_schema_v1.rs"]
mod inbox_schema_v1;
#[path = "remote_repository_v1.rs"]
mod remote_repository_v1;
#[path = "repository_catalogue_scope_stops_v1.rs"]
mod scope_stops_v1;
#[path = "repository_catalogue_scoped_pr_read_v1.rs"]
pub(crate) mod scoped_pr_read_v1;
#[path = "repository_catalogue_scoped_refresh_v1.rs"]
mod scoped_refresh_v1;

fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", endpoint, "--hostname", HOST, "--method", "GET"],
        &value.to_string(),
    )
}
fn principal(uid: u64) -> Vec<GhStep> {
    vec![
        // Actual release syntax, not the old permissive offline-fixture string.
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":"octocat"})),
    ]
}
fn owner() -> Value {
    json!({"id":71,"node_id":"OWNER_catalogue_71","login":"example","type":"Organization"})
}
fn repository(index: u64) -> Value {
    json!({"id":10000+index,"node_id":format!("REPO_catalogue_{}",10000+index),
        "full_name":format!("example/repo{index:04}"),"owner":owner(),
        "description":"Private bounded discovery fixture","private":true,
        "fork":false,"archived":false,"default_branch":"main","language":"Rust",
        "permissions":{"pull":true,"push":false},"updated_at":"2026-10-01T00:00:00Z"})
}
fn endpoint(page: usize) -> String {
    format!("orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page={page}")
}
fn collection(pages: Vec<Value>, final_uid: u64) -> Vec<GhStep> {
    let mut result = principal(42);
    result.push(get("orgs/example", owner()));
    for (index, page) in pages.into_iter().enumerate() {
        result.push(get(&endpoint(index + 1), page));
    }
    result.push(get("orgs/example", owner()));
    result.extend(principal(final_uid));
    result
}
fn thousand_one() -> Vec<GhStep> {
    let pages = (0..11)
        .map(|page| {
            json!((page * 100..((page + 1) * 100).min(1001))
                .map(repository)
                .collect::<Vec<_>>())
        })
        .collect();
    let result = collection(pages, 42);
    assert_eq!(result.len(), 17, "11 fixed pages plus six owned proofs");
    result
}
fn save_intent() -> Vec<GhStep> {
    // Metadata intent only: original producer/SQL epoch is checked by Root;
    // these reads do not refresh membership or grant mutation authority.
    let mut result = principal(42);
    result.extend(principal(42));
    assert_eq!(result.len(), 4);
    result
}
fn activate_two() -> Vec<GhStep> {
    let mut result = principal(42);
    // Canonical sorted original ID order, never display/filter order.
    result.push(get(&format!("repos/{FIRST}"), repository(0)));
    result.push(get(&format!("repos/{LAST}"), repository(1000)));
    result.extend(principal(42));
    assert_eq!(result.len(), 6);
    result
}
fn open_my_prs() -> Vec<GhStep> {
    let mut result = principal(42);
    result.push(get(&format!("repos/{LAST}"), repository(1000)));
    // Existing Inbox refresh has its OWN identity reads; do not hide them in
    // a nine-call fixture or reinterpret an empty repo scope as all repositories.
    result.extend(principal(42));
    for qualifier in ["author", "assignee", "review-requested"] {
        result.push(GhStep::read(
            &[
                "api",
                "search/issues",
                "--hostname",
                HOST,
                "--method",
                "GET",
                "-f",
                &format!("q=is:pr is:open {qualifier}:octocat repo:{LAST}"),
                "-f",
                "per_page=100",
                "-f",
                "page=1",
            ],
            "{\"total_count\":0,\"incomplete_results\":false,\"items\":[]}",
        ));
    }
    result.push(get(&format!("repos/{LAST}"), repository(1000)));
    result.extend(principal(42));
    assert_eq!(result.len(), 11);
    result
}
fn verify_unknown_access() -> Vec<GhStep> {
    let mut result = principal(42);
    let mut current = repository(1000);
    current["permissions"] = Value::Null;
    result.push(get(&format!("repos/{LAST}"), current));
    // Exact successful GET is not converted to an observed pull flag. The
    // closing account check retains no new permission/body/membership claim.
    result.extend(principal(42));
    assert_eq!(result.len(), 5);
    result
}

#[derive(Debug, PartialEq, Eq)]
struct GitState {
    head: String,
    refs: String,
    index: Vec<u8>,
    status: String,
    staged_diff: String,
    working_diff: String,
    dirty: Vec<u8>,
    staged: Vec<u8>,
    untracked: Vec<u8>,
}
fn git_state(fixture: &Fixture) -> GitState {
    GitState {
        head: fixture.git(&["rev-parse", "HEAD"]),
        refs: fixture.git(&["show-ref"]),
        index: fs::read(fixture.repo().join(".git/index")).unwrap(),
        status: fixture.git(&["status", "--porcelain=v2", "-z"]),
        staged_diff: fixture.git(&["diff", "--cached", "--binary", "--no-ext-diff"]),
        working_diff: fixture.git(&["diff", "--binary", "--no-ext-diff"]),
        dirty: fs::read(fixture.repo().join("dirty界.txt")).unwrap(),
        staged: fs::read(fixture.repo().join("staged.txt")).unwrap(),
        untracked: fs::read(fixture.repo().join("scratch.txt")).unwrap(),
    }
}
fn database(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("config/ghtui/cache.sqlite3")
}
fn configured(steps: &[GhStep]) -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "dirty界.txt",
        b"original checkout bytes\n",
        "Catalogue baseline",
    );
    fixture.commit_file(
        "staged.txt",
        b"original index bytes\n",
        "Later catalogue version",
    );
    fixture.git(&["tag", "catalogue-original"]);
    let bare = fixture.root.join("catalogue-local-remote.git");
    fixture.git(&["init", "--bare", bare.to_str().unwrap()]);
    fixture.git(&["remote", "add", "origin", bare.to_str().unwrap()]);
    fs::write(fixture.repo().join("dirty界.txt"), "dirty checkout α\n").unwrap();
    fs::write(fixture.repo().join("staged.txt"), b"staged chosen bytes\n").unwrap();
    fixture.git(&["add", "--", "staged.txt"]);
    fs::write(
        fixture.repo().join("scratch.txt"),
        b"untracked discovery bytes\n",
    )
    .unwrap();
    Config {
        host: HOST.into(),
        avatars: false,
        // Discovered names/sets MUST NOT be saved in this global config.
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    Cache::open(&database(&fixture))
        .unwrap()
        .put_draft(
            &Scope::github(HOST, Some(42)).unwrap(),
            "catalogue-draft",
            "Original42 draft",
        )
        .unwrap();
    fixture.scripted_gh(steps);
    fixture
}
fn launch(fixture: &Fixture, mouse: bool) -> Session {
    let trace = fixture.root.join("catalogue-git-trace.jsonl");
    let env = if mouse {
        vec![
            ("GIT_TRACE2_EVENT", trace.to_str().unwrap()),
            ("NO_COLOR", "1"),
        ]
    } else {
        vec![("GIT_TRACE2_EVENT", trace.to_str().unwrap())]
    };
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        fixture,
        if mouse { 80 } else { 120 },
        if mouse { 24 } else { 40 },
        &env,
    );
    session.wait_for_screen("Recent commits");
    session.assert_focused_view("Repository");
    session
}
fn commands(session: &Session, fixture: &Fixture) -> Vec<Value> {
    session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn assert_commands(session: &Session, fixture: &Fixture, expected: usize) {
    let rows = commands(session, fixture);
    assert_eq!(rows.len(), expected);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    assert!(rows
        .iter()
        .all(|row| row["args"][0] == "--version" || row["args"][5] == "GET"));
    assert!(!rows.iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "--paginate" || arg == "--slurp" || arg == "graphql")));
}
fn git_commands(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("catalogue-git-trace.jsonl"))
        .expect("Missing actual child Git instrumentation is not zero commands")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|row: &Value| row["event"] == "start")
        .collect()
}
fn assert_read_only_git(fixture: &Fixture) {
    let trace = fs::read_to_string(fixture.root.join("catalogue-git-trace.jsonl")).unwrap();
    let rows = trace
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(rows.iter().any(|row| row["event"] == "cmd_name"));
    assert!(!rows
        .iter()
        .filter(|row| row["event"] == "cmd_name")
        .any(|row| matches!(
            row["name"].as_str(),
            Some(
                "add"
                    | "reset"
                    | "commit"
                    | "checkout"
                    | "switch"
                    | "update-ref"
                    | "fetch"
                    | "clone"
                    | "pull"
                    | "push"
                    | "merge"
                    | "rebase"
                    | "stash"
                    | "rm"
            )
        )));
}
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
/// Refresh by key, or by clicking its key-bar token. Narrow bars drop the token by priority
/// (Search at 80 columns); the command palette then runs the same global Refresh by mouse.
fn refresh_control(session: &mut Session, mouse: bool) {
    if !mouse {
        session.send_key(b"r");
    } else if session
        .try_wait_for_control("[r Refresh]", Duration::from_millis(500))
        .is_some()
    {
        session.click_visible_action("[r Refresh]");
    } else {
        session.invoke_via_palette(true, "[r Refresh]");
    }
}
/// Tab until the painted Quit control (the catalogue's `[q Quit]` button, or the main key-bar
/// `q Quit` token) shows keyboard focus, re-reading its position after every completed frame.
fn focus_quit(session: &mut Session) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(50));
        let (column, row) = session.wait_for_control("[q Quit]");
        if session.cell_underlined(row, column) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Quit never gained keyboard focus\n{}",
            session.screen()
        );
        session.send_key(b"\t");
    }
}
/// The catalogue closed onto the actual current main frame: a newer completed Repository frame
/// with the main key bar (`? Help` and `q Quit` tokens in the last row), no catalogue or notice.
fn restored_main(frame: Option<&vt100::Screen>, completed_end: usize, before_close: usize) -> bool {
    completed_end > before_close
        && frame.is_some_and(|frame| {
            let text = frame.contents();
            text.contains("Repository • focused view")
                && !text.contains(TITLE)
                && !text.contains("Action unavailable • explanation only")
                && crate::support::controls::main_footer_visible(frame)
        })
}
fn ready(session: &mut Session, required: &[&str]) {
    session
        .try_wait_for_condition(
            |screen| {
                required
                    .iter()
                    .all(|label| crate::support::controls::text_has_control(screen, label))
            },
            &format!("Actual complete catalogue frame with {required:?}"),
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
}
/// The focused Inbox frame title carries `host / @login / repository / queue`.
/// Narrow frames ellipsize the repository from the left, so accept only its
/// full form or a `…` tail that still shows the complete `/name` segment. The
/// list pane beside the 120-column Preview has room for neither, and its title
/// then drops the whole repository level (`… / queue`); the journeys pin the
/// repository through their exact command journals.
fn inbox_scope_title(screen: &str, repository: &str, queue: &str) -> bool {
    let name = &repository[repository.find('/').unwrap()..];
    screen.lines().take(3).any(|line| {
        let Some(start) = line.find("Inbox • focused view") else {
            return false;
        };
        let Some(after) = line[start..].split(&format!("{HOST} / @octocat / ")).nth(1) else {
            return false;
        };
        let Some((shown, _)) = after.split_once(&format!(" / {queue} ")) else {
            return false;
        };
        shown == repository
            || (shown == "…" && line.contains("╮╭─ Preview "))
            || shown.strip_prefix('…').is_some_and(|tail| {
                tail.len() >= name.len() && repository.ends_with(tail) && tail.ends_with(name)
            })
    })
}
fn overlay_ready(session: &mut Session, title: &str, required: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(10));
        if session
            .visible_overlay_body(title)
            .is_some_and(|body| required.iter().all(|label| body.contains(label)))
        {
            return;
        }
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "Actual overlay body not ready: {title} requires {required:?}\n{}",
                session.screen()
            );
        }
    }
}
fn open_catalogue(session: &mut Session, fixture: &Fixture, mouse: bool, expected: usize) {
    choose(session, mouse, "[: Commands]", b":");
    overlay_ready(
        session,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    session.send_key(b"\x01");
    session.paste("Discover repositories");
    overlay_ready(
        session,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(session, mouse, "> Discover repositories", b"\r");
    session.wait_for_overlay_close("Commands · current context");
    ready(session, &[TITLE, "[s Source]", "[Esc Close]"]);
    assert_commands(session, fixture, expected);
}
fn discover(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[s Source]", b"s");
    overlay_ready(
        session,
        SOURCE,
        &[
            "[o Organization]",
            "[u My accessible repositories]",
            "[p Public user]",
            "Owner",
            "[Enter Discover]",
        ],
    );
    choose(session, mouse, "[o Organization]", b"o");
    if mouse {
        session.click_visible_action("Owner");
    } else {
        session.send_key(b"\t");
    }
    session.send_key(b"\x01");
    session.paste("example");
    overlay_ready(session, SOURCE, &["example", "[Enter Discover]"]);
    choose(session, mouse, "[Enter Discover]", b"\r");
    session.wait_for_overlay_close(SOURCE);
}
fn filter(session: &mut Session, mouse: bool, query: &str, name: &str, id: &str) {
    choose(session, mouse, "[/ Filter]", b"/");
    session.send_key(b"\x01");
    session.paste(query);
    // Existing rows are visible before this asynchronous query. Observe the
    // genuine current prepared result, not an old matching name/ID, before
    // sending a selection-dependent action through either input path.
    ready(
        session,
        &[
            TITLE,
            &format!("Filter focused: {query}"),
            "matches 1",
            name,
            id,
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
                    && screen.contains(name)
                    && screen.contains(id)
                    && !screen.contains("Filter focused:")
            },
            "Actual current catalogue list focus before target action",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn save(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[w Save set]", b"w");
    overlay_ready(session, SAVE, &["Set name", "[Enter Save]"]);
    session.send_key(b"\x01");
    session.paste(SET);
    choose(session, mouse, "[Enter Save]", b"\r");
    session.wait_for_overlay_close(SAVE);
    ready(session, &[TITLE, "Saved observed set", SET]);
}
fn activate(session: &mut Session, mouse: bool) {
    if !mouse {
        // Discovery opens with its text filter focused. Move to the list before
        // using its action shortcut; an editor's 'a' must remain editable text.
        session.send_key(b"\t");
        ready(session, &[TITLE, "Filter:"]);
    }
    choose(session, mouse, "[a Activate set]", b"a");
    overlay_ready(session, ACTIVATE, &[SET, "[Enter Activate]"]);
    choose(session, mouse, "[Enter Activate]", b"\r");
    session.wait_for_overlay_close(ACTIVATE);
    ready(session, &[TITLE, "Active explicit set", SET]);
}
fn pure_controls(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize) {
    let gh = session.inspect_command_journal(fixture);
    let git = git_commands(fixture);
    // Actual list focus and Unicode no-match/filter/caret transitions, not a
    // direct controller call or an accepted-by-default screen snapshot.
    choose(session, mouse, "[/ Filter]", b"/");
    session.send_key(b"\x01");
    session.paste("absent-界-🦀");
    ready(
        session,
        &[TITLE, "No matching repositories", "absent-界-🦀"],
    );
    session.send_key(b"\x01");
    session.send_key(b"\x7f");
    ready(session, &[FIRST, "Repository ID: 10000"]);
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        TITLE,
        &["[q Quit]", "[Esc Close]"],
    );
    ready(session, &[TITLE, FIRST, "Repository ID: 10000"]);
    let (column, row) = session
        .locate("◆ repository · example/repo0000")
        .expect("Latest resized actual result geometry, not the filter text");
    if mouse {
        session.click(column, row);
        session.wheel(column, row, true);
        ready(session, &["Repository ID: 10003"]);
        session.wheel(column, row, false);
    } else {
        session.send_key(b"\t\x1b[B\x1b[B\x1b[B");
        ready(session, &["Repository ID: 10003"]);
        session.send_key(b"\x1b[A\x1b[A\x1b[A");
    }
    ready(session, &[TITLE, FIRST, "Repository ID: 10000"]);
    choose(session, mouse, "[PgDn]", b"\x1b[6~");
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains(TITLE)
                    && screen.contains("Repository ID:")
                    && !screen.contains("Repository ID: 10000")
            },
            "Actual page movement to a different canonical repository detail",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    choose(session, mouse, "[PgUp]", b"\x1b[5~");
    ready(session, &[TITLE, FIRST, "Repository ID: 10000"]);
    // Actual detail-page transitions use the shared key/click action and return
    // to the original selected identity before querying the1001st member.
    choose(session, mouse, "[Ctrl-PgDn Detail]", b"\x1b[6;5~");
    ready(session, &["Pull permission: true"]);
    choose(session, mouse, "[Ctrl-PgUp Detail]", b"\x1b[5;5~");
    ready(session, &["Repository ID: 10000"]);
    assert_eq!(session.inspect_command_journal(fixture), gh);
    assert_eq!(
        git_commands(fixture),
        git,
        "Prepared catalogue input must not invoke Git"
    );
    assert_commands(session, fixture, count);
}
fn assert_intent(fixture: &Fixture, ids: &[u64]) -> Value {
    let cache = Cache::open(&database(fixture)).unwrap();
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let saved = cache.saved_views(&scope).unwrap();
    // Proposed bounded intent ABI, never interpreted as auth by the application.
    let values = saved
        .iter()
        .filter(|(_, value)| value["sets"][SET].is_object())
        .collect::<Vec<_>>();
    assert_eq!(
        values.len(),
        1,
        "One genuine original-scope set, not name-keyed global config"
    );
    let value = &values[0].1;
    assert_eq!(value["version"], 1);
    assert_eq!(value["scope"]["host"], scope.host);
    assert_eq!(value["scope"]["login"], scope.login);
    assert_eq!(value["source"]["owner_id"], 71);
    assert_eq!(value["source"]["owner_node_id"], "OWNER_catalogue_71");
    let members = value["sets"][SET].as_object().unwrap();
    assert_eq!(
        members.len(),
        ids.len(),
        "No selected-set expansion to all1001"
    );
    for id in ids {
        let reference = &members[&id.to_string()];
        assert_eq!(reference["id"], *id);
        assert_eq!(reference["node_id"], format!("REPO_catalogue_{id}"));
        assert_eq!(
            reference["last_name"],
            format!("example/repo{:04}", id - 10000)
        );
    }
    if ids.contains(&11000) {
        assert_eq!(value["pins"]["11000"]["id"], 11000);
    }
    assert_eq!(
        cache.draft(&scope, "catalogue-draft").unwrap().as_deref(),
        Some("Original42 draft")
    );
    let other = Scope::github(HOST, Some(43)).unwrap();
    assert!(cache.saved_views(&other).unwrap().is_empty());
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.draft(&other, "catalogue-draft").unwrap().is_none());
    value.clone()
}
fn finish(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    count: usize,
    before: &GitState,
    config: &[u8],
) {
    session.quit_and_assert_restored_terminal(mouse);
    assert_commands(session, fixture, count); // Includes any late/resend work.
    assert_read_only_git(fixture);
    assert_eq!(git_state(fixture), *before);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
}

#[test]
fn repo_catalog_native_keyboard_1001_filter_select_pin_save_restart_activate_exact_my_prs() {
    happy(false);
}
#[test]
fn repo_catalog_native_mouse_1001_filter_select_pin_save_restart_activate_exact_my_prs() {
    happy(true);
}
fn happy(mouse: bool) {
    let mut steps = thousand_one();
    steps.extend(save_intent());
    steps.extend(thousand_one());
    steps.extend(activate_two());
    steps.extend(open_my_prs());
    assert_eq!(
        steps.len(),
        55,
        "Declared21 then34 commands across two real sessions"
    );
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 1001",
            "API-visible membership complete",
            "non-atomic",
        ],
    );
    assert_commands(&session, &fixture, 17);
    pure_controls(&mut session, &fixture, mouse, 17);
    filter(&mut session, mouse, LAST, LAST, "Repository ID: 11000");
    choose(&mut session, mouse, "[Space Select]", b" ");
    choose(&mut session, mouse, "[p Pin]", b"p");
    ready(&mut session, &[LAST, "Selected: yes", "Pinned: yes"]);
    filter(&mut session, mouse, FIRST, FIRST, "Repository ID: 10000");
    choose(&mut session, mouse, "[Space Select]", b" ");
    ready(&mut session, &[FIRST, "Selected: yes"]);
    save(&mut session, mouse);
    assert_commands(&session, &fixture, 21);
    let original = assert_intent(&fixture, &[10000, 11000]);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    // The body may be repainted before the owned overlay footer is cleared.
    // Observe actual restored footer cells before targeting the Quit control.
    session.wait_for_main_frame();
    session.wait_for_screen("Repository • focused view");
    finish(&mut session, &fixture, mouse, 21, &before, &config);

    let mut restarted = launch(&fixture, mouse);
    open_catalogue(&mut restarted, &fixture, mouse, 21);
    discover(&mut restarted, mouse); // No private cached names before fresh scope.
    ready(
        &mut restarted,
        &[TITLE, "Observed: 1001", "API-visible membership complete"],
    );
    assert_commands(&restarted, &fixture, 38);
    assert_eq!(assert_intent(&fixture, &[10000, 11000]), original);
    activate(&mut restarted, mouse);
    assert_commands(&restarted, &fixture, 44);
    filter(&mut restarted, mouse, LAST, LAST, "Repository ID: 11000");
    let background = restarted
        .locate("3 Search")
        .expect("Underlying tab for overlay capture");
    if mouse {
        restarted.click(background.0, background.1);
    }
    ready(&mut restarted, &[TITLE, LAST, "Active explicit set"]);
    assert_commands(&restarted, &fixture, 44);
    choose(&mut restarted, mouse, "[Open My PRs]", b"\r");
    restarted
        .try_wait_for_condition(
            |screen| {
                inbox_scope_title(screen, LAST, "My PRs")
                    && [
                        "PULL REQUESTS",
                        "0 unique PRs across 0 queue matches",
                        "No pull requests in this queue / scope.",
                    ]
                    .iter()
                    .all(|label| screen.contains(label))
            },
            "Exact LAST My PRs inbox in the focused frame title",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    assert_commands(&restarted, &fixture, 55);
    assert_eq!(assert_intent(&fixture, &[10000, 11000]), original);
    finish(&mut restarted, &fixture, mouse, 55, &before, &config);
}

#[test]
fn repo_catalog_native_keyboard_partial_rate_failure_unknown_fields_explicit_retry_test_framework_catalogue_close_frame_v1(
) {
    partial(false);
}
#[test]
fn repo_catalog_native_mouse_partial_rate_failure_unknown_fields_explicit_retry_test_framework_catalogue_close_frame_v1(
) {
    partial(true);
}
fn partial(mouse: bool) {
    let first = json!((0..100).map(repository).collect::<Vec<_>>());
    let mut initial = collection(vec![first.clone(), json!([])], 42);
    initial[4].status = 1;
    initial[4].stdout.clear();
    initial[4].stderr = b"HTTP 429: rate limit exceeded\n".to_vec();
    let mut nullable = repository(1000);
    for field in [
        "owner",
        "private",
        "archived",
        "fork",
        "language",
        "default_branch",
        "permissions",
    ] {
        nullable.as_object_mut().unwrap().remove(field);
    }
    let mut steps = initial;
    steps.extend(collection(vec![first, json!([nullable])], 42));
    steps.extend(verify_unknown_access());
    assert_eq!(
        steps.len(),
        21,
        "Two explicit8-read traversals plus five explicit access reads, no automatic retry"
    );
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 100",
            "API-visible membership partial",
            "RateLimited",
            "[r Retry]",
        ],
    );
    assert!(!session.screen().contains("Observed: 0"));
    assert_commands(&session, &fixture, 8);
    // Hidden/unseen row cannot be opened as a completed empty result.
    choose(&mut session, mouse, "[/ Filter]", b"/");
    session.send_key(b"\x01");
    session.paste(LAST);
    ready(
        &mut session,
        &[
            TITLE,
            "No matching repositories",
            "API-visible membership partial",
        ],
    );
    assert_commands(&session, &fixture, 8);
    if !mouse {
        session.send_key(b"\t");
    }
    choose(&mut session, mouse, "[r Retry]", b"r");
    ready(
        &mut session,
        &[
            TITLE,
            LAST,
            "Observed: 101",
            "API-visible membership complete",
            "Repository ID: 11000",
        ],
    );
    ready(&mut session, &["Pull permission: unknown"]);
    // Narrow layouts have fewer visible detail lines after additional controls.
    // Scroll the real detail panel instead of requiring both facts in one page.
    choose(&mut session, mouse, "[Ctrl-PgDn Detail]", b"\x1b[6;5~");
    ready(&mut session, &["Archive state: unknown"]);
    choose(&mut session, mouse, "[Ctrl-PgUp Detail]", b"\x1b[5;5~");
    ready(
        &mut session,
        &["Repository ID: 11000", "Pull permission: unknown"],
    );
    assert_commands(&session, &fixture, 16);
    // Omitted permission is NOT false or a denial. Explicit Open performs the
    // guarded current-access read; still-unobserved pull cannot unlock queries.
    choose(&mut session, mouse, "[Open My PRs]", b"\r");
    overlay_ready(
        &mut session,
        "Action unavailable • explanation only",
        &["not observed", "permission"],
    );
    if mouse {
        session.close_overlay_by_mouse("Action unavailable • explanation only");
    } else {
        session.close_overlay_by_key("Action unavailable • explanation only");
    }
    ready(&mut session, &[TITLE, LAST, "Repository ID: 11000"]);
    assert_commands(&session, &fixture, 21);
    let before_close = session.completed_cursorless_end();
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if restored_main(
            session.completed_cursorless_screen(),
            session.completed_cursorless_end(),
            before_close,
        ) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Catalogue close did not complete the actual current main toolbar"
        );
    }
    let completed_quit = crate::support::controls::locate_control(
        session.completed_cursorless_screen().unwrap(),
        "[q Quit]",
    )
    .expect("Current completed main Quit control, not an old catalogue hit");
    assert_eq!(
        crate::support::controls::footer_control(
            session.completed_cursorless_screen().unwrap(),
            &crate::support::controls::Control::parse("[q Quit]"),
        ),
        Some(completed_quit),
        "Quit is the main key-bar token, not the closed catalogue's bracketed button"
    );
    assert_eq!(session.wait_for_control("[q Quit]"), completed_quit);
    finish(&mut session, &fixture, mouse, 21, &before, &config);
}

/// TASK-9: `u My accessible repositories` and `p Public user` page through 101 rows each
/// (a full page of 100, then a short page), chosen by key or click.
fn user_inventories() -> Vec<GhStep> {
    let page = |range: std::ops::Range<u64>, public: bool| {
        json!(range
            .map(|index| {
                let mut row = repository(index);
                if public {
                    row["private"] = json!(false);
                    row["owner"] = user();
                }
                row
            })
            .collect::<Vec<_>>())
    };
    let mine = |page: usize| {
        format!("user/repos?visibility=all&affiliation=owner,collaborator,organization_member&sort=full_name&direction=asc&per_page=100&page={page}")
    };
    let public = |page: usize| {
        format!(
            "users/example/repos?type=owner&sort=full_name&direction=asc&per_page=100&page={page}"
        )
    };
    let mut result = principal(42);
    result.push(get(&mine(1), page(0..100, false)));
    result.push(get(&mine(2), page(100..101, false)));
    result.extend(principal(42));
    result.extend(principal(42));
    result.push(get("users/example", user()));
    result.push(get(&public(1), page(2000..2100, true)));
    result.push(get(&public(2), page(2100..2101, true)));
    result.push(get("users/example", user()));
    result.extend(principal(42));
    assert_eq!(result.len(), 14);
    result
}
fn user() -> Value {
    json!({"id":72,"node_id":"OWNER_catalogue_72","login":"example","type":"User"})
}
#[test]
fn repo_catalog_native_keyboard_my_accessible_and_public_user_sources_page_101_rows() {
    user_sources(false);
}
#[test]
fn repo_catalog_native_mouse_my_accessible_and_public_user_sources_page_101_rows() {
    user_sources(true);
}
fn user_sources(mouse: bool) {
    let fixture = configured(&user_inventories());
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    choose(&mut session, mouse, "[s Source]", b"s");
    overlay_ready(&mut session, SOURCE, &["[u My accessible repositories]"]);
    choose(&mut session, mouse, "[u My accessible repositories]", b"u");
    overlay_ready(
        &mut session,
        SOURCE,
        &["Owner not used for My accessible repositories"],
    );
    choose(&mut session, mouse, "[Enter Discover]", b"\r");
    session.wait_for_overlay_close(SOURCE);
    ready(
        &mut session,
        &[TITLE, "Observed: 101", "Source: My accessible", FIRST],
    );
    assert_commands(&session, &fixture, 6);
    leave_filter(&mut session, mouse);
    choose(&mut session, mouse, "[s Source]", b"s");
    overlay_ready(&mut session, SOURCE, &["[p Public user]"]);
    choose(&mut session, mouse, "[p Public user]", b"p");
    if mouse {
        session.click_visible_action("Owner");
    } else {
        session.send_key(b"\t");
    }
    session.send_key(b"\x01");
    session.paste("example");
    overlay_ready(&mut session, SOURCE, &["example", "[Enter Discover]"]);
    choose(&mut session, mouse, "[Enter Discover]", b"\r");
    session.wait_for_overlay_close(SOURCE);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 101",
            "Source: Public user example",
            "example/repo2000",
        ],
    );
    assert!(!session.screen().contains(FIRST), "{}", session.screen());
    assert_commands(&session, &fixture, 14);
    // Discovery saves nothing: no repository set, and the other account sees nothing.
    let cache = Cache::open(&database(&fixture)).unwrap();
    assert!(cache
        .saved_views(&Scope::github(HOST, Some(42)).unwrap())
        .unwrap()
        .iter()
        .all(|(_, value)| value["sets"][SET].is_null()));
    leave_filter(&mut session, mouse);
    finish(&mut session, &fixture, mouse, 14, &before, &config);
}
#[test]
fn repo_catalog_native_keyboard_kind_filter_archived_fork_select_save_exact_intent() {
    kind_filter(false);
}
#[test]
fn repo_catalog_native_mouse_kind_filter_archived_fork_select_save_exact_intent() {
    kind_filter(true);
}
/// The archive/fork narrowing is live: it re-prepares only the observed rows,
/// excludes omitted (unknown) facts, issues no GitHub or Git command, and a
/// Save keeps exactly the explicitly selected narrowed original.
fn kind_filter(mouse: bool) {
    let mut archived = repository(1);
    archived["archived"] = json!(true);
    let mut fork = repository(2);
    fork["fork"] = json!(true);
    let mut unknown = repository(3);
    unknown["archived"] = Value::Null;
    unknown["fork"] = Value::Null;
    let mut steps = collection(vec![json!([repository(0), archived, fork, unknown])], 42);
    steps.extend(save_intent());
    assert_eq!(steps.len(), 11, "Source7 then Save4");
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 4",
            "API-visible membership complete",
            "matches 4",
            "[f Kind: All]",
        ],
    );
    assert_commands(&session, &fixture, 7);
    let git = git_commands(&fixture);
    leave_filter(&mut session, mouse);
    let row = |index: u64| format!("◆ repository · example/repo{index:04}");
    let shows = |screen: &str, visible: &[u64], hidden: &[u64]| {
        visible.iter().all(|index| screen.contains(&row(*index)))
            && hidden.iter().all(|index| !screen.contains(&row(*index)))
    };
    choose(&mut session, mouse, "[f Kind: All]", b"f");
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("[f Kind: Not archived]")
                    && screen.contains("matches 2 • kind Not archived")
                    && shows(screen, &[0, 2], &[1, 3])
            },
            "Not archived keeps known false archive facts only",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    choose(&mut session, mouse, "[f Kind: Not archived]", b"f");
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("[f Kind: Archived]")
                    && screen.contains("matches 1 • kind Archived")
                    && screen.contains("Repository ID: 10001")
                    && shows(screen, &[1], &[0, 2, 3])
            },
            "Archived narrows to the observed archived repository",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    choose(&mut session, mouse, "[Space Select]", b" ");
    ready(&mut session, &["Repository ID: 10001", "Selected: yes"]);
    choose(&mut session, mouse, "[f Kind: Archived]", b"f");
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("[f Kind: Forks]")
                    && screen.contains("matches 1 • kind Forks")
                    && screen.contains("Repository ID: 10002")
                    && screen.contains("Selected: no")
                    && shows(screen, &[2], &[0, 1, 3])
            },
            "Forks narrows to the observed fork",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    // Narrowing is presentation over the original read: no GitHub or Git work.
    assert_commands(&session, &fixture, 7);
    assert_eq!(git_commands(&fixture), git);
    save(&mut session, mouse);
    assert_commands(&session, &fixture, 11);
    assert_intent(&fixture, &[10001]);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_main_frame();
    session.wait_for_screen("Repository • focused view");
    finish(&mut session, &fixture, mouse, 11, &before, &config);
}

/// A finished discovery leaves the filter focused; Tab moves to the list so keys are actions.
fn leave_filter(session: &mut Session, mouse: bool) {
    if mouse {
        return;
    }
    session.send_key(b"\t");
    session
        .try_wait_for_condition(
            |screen| screen.contains(TITLE) && !screen.contains("Filter focused:"),
            "catalogue list focus after discovery",
            Duration::from_secs(8),
        )
        .unwrap();
}

#[test]
fn repo_catalog_native_keyboard_final_uid43_reused_login_hides_private_results_and_hits() {
    principal_change(false);
}
#[test]
fn repo_catalog_native_mouse_final_uid43_reused_login_hides_private_results_and_hits() {
    principal_change(true);
}
fn principal_change(mouse: bool) {
    let mut steps = collection(vec![json!([repository(1000)])], 42);
    steps.extend(save_intent());
    steps.extend(collection(vec![json!([repository(1000)])], 43));
    assert_eq!(steps.len(), 18);
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    ready(
        &mut session,
        &[TITLE, LAST, "Observed: 1", "Repository ID: 11000"],
    );
    assert_commands(&session, &fixture, 7);
    filter(&mut session, mouse, LAST, LAST, "Repository ID: 11000");
    choose(&mut session, mouse, "[Space Select]", b" ");
    choose(&mut session, mouse, "[p Pin]", b"p");
    ready(&mut session, &[LAST, "Selected: yes", "Pinned: yes"]);
    save(&mut session, mouse);
    assert_commands(&session, &fixture, 11);
    let original = assert_intent(&fixture, &[11000]);
    let old = session
        .locate("[Open My PRs]")
        .expect("Original admitted target hit");
    choose(&mut session, mouse, "[r Retry]", b"r");
    session.try_wait_for_condition(|screen|screen.contains("private repositories hidden")
        && !screen.contains(LAST) && !screen.contains("REPO_catalogue_11000")
        && !screen.contains("Observed: 1") && !screen.contains("Repository ID: 11000")
        && !screen.contains("Selected: yes") && !screen.contains("@example"),
        "Final numeric43 same-login complete hidden frame, no previous private row/count/actor/ID",
        Duration::from_secs(8)).unwrap_or_else(|error|panic!("{error}"));
    assert_commands(&session, &fixture, 18);
    if mouse {
        session.click(old.0.min(75), old.1.min(20));
    } else {
        session.send_key(b"\r");
    }
    // Input processed, not merely queued: Tab reaches Quit, whether the catalogue's own
    // bracketed button (still open) or the main key-bar token (closed by keyboard Enter).
    focus_quit(&mut session);
    assert!(!session.screen().contains(LAST));
    assert_commands(&session, &fixture, 18);
    assert_eq!(
        assert_intent(&fixture, &[11000]),
        original,
        "Original42 intent remains private and ID-bound, never scope43 reuse"
    );
    finish(&mut session, &fixture, mouse, 18, &before, &config);
}
