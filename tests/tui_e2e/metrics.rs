//! Registered strict native Dashboard scope/window/evidence and access-loss
//! journeys under active verification. TASK-61 records actual results/artifacts;
//! focused passes do not imply complete metric/history or platform coverage.
use crate::hydration_fixture as gh_fixture;
use crate::support::{Fixture, GhStep, Session};
use ghtui::{
    config::Config,
    storage::{Cache, Scope},
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, time::Duration};

#[path = "metrics_boundaries.rs"]
mod boundaries;
#[path = "dashboard_origin_v1.rs"]
mod dashboard_origin_v1;
#[path = "dashboard_trends_v1.rs"]
mod dashboard_trends_v1;
#[path = "dashboards_v1.rs"]
mod dashboards_v1;
#[path = "design_footer_v90.rs"]
mod design_footer_v90;

const APP: &str = "example/app";
const TOOLS: &str = "example/tools";
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BASE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TITLE: &str = "Review guarded auth";
const BODY: &str = "Canonical Dashboard PR body opened read-only";
const DRAFT: &str = "Draft cache migration";
const CARDS: [&str; 8] = [
    "Loaded open PRs",
    "Needs my review",
    "Mean open PR age",
    "Mean requested-review wait",
    "Mean first submitted review latency",
    "Mean merge latency",
    "Observed exact-head check health",
    "Latest observed workflow-attempt health",
];
const FORMULAS: [&str; 8] = [
    "including drafts",
    "non-draft PRs",
    "original created_at",
    "latest outstanding request",
    "first submitted review UTC",
    "verified merged-event UTC",
    "required-rule completeness",
    "highest observed attempt",
];

/// One test-data clock, never an application clock override. The same value
/// reaches every inventory and canonical Open response in this journey.
#[derive(Clone, Copy)]
struct FixtureUtcAnchorV43(u64);
impl FixtureUtcAnchorV43 {
    fn capture() -> Self {
        Self(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
    }
    fn alpha_created(self) -> u64 {
        self.0.checked_sub(3 * 86_400).unwrap()
    }
    fn bot_created(self) -> u64 {
        self.0.checked_sub(15 * 86_400).unwrap()
    }
    fn timestamp(seconds: u64) -> String {
        format!("{}Z", utc(seconds).replace(' ', "T"))
    }
    fn stamp_updated(self, value: &mut Value) {
        value["updated_at"] = json!(Self::timestamp(self.0.checked_sub(86_400).unwrap()));
    }
}

fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
fn configured(two: bool) -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "tracked.txt",
        b"original Dashboard fixture bytes\n",
        "Original Dashboard history",
    );
    fs::write(
        fixture.repo().join("tracked.txt"),
        b"local dirty Dashboard bytes\n",
    )
    .unwrap();
    let names = if two {
        vec![APP.into(), TOOLS.into()]
    } else {
        vec![APP.into()]
    };
    Config {
        host: gh_fixture::HOST.into(),
        avatars: false,
        active_set: Some("dashboard-explicit".into()),
        repository_sets: BTreeMap::from([("dashboard-explicit".into(), names)]),
        local_repositories: BTreeMap::from([(APP.into(), fixture.repo())]),
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    fixture
}
fn repository(name: &str) -> Value {
    let id = if name == APP { 410 } else { 420 };
    json!({"id":id,"node_id":format!("R_dashboard_{id}"),"full_name":name,"url":format!("https://{}/api/v3/repos/{name}",gh_fixture::HOST),"html_url":format!("https://{}/{name}",gh_fixture::HOST),"private":true,"default_branch":"main","archived":false,"permissions":{"pull":true,"push":true}})
}
fn pull(
    name: &str,
    number: u64,
    draft: bool,
    created: Option<&str>,
    actor: Value,
    reviewer: u64,
) -> Value {
    let title = match number {
        1 => TITLE,
        8 => "Bounded bot housekeeping",
        _ => DRAFT,
    };
    json!({"id":1000+number,"node_id":format!("PR_dashboard_{number}"),"number":number,"title":title,"body":if number==1 {BODY} else {"Fixture body is not a metric timestamp"},"state":"open","draft":draft,
        "created_at":created,"updated_at":"2026-10-13T00:00:00Z","closed_at":null,"merged_at":null,"merged":false,
        "url":format!("https://{}/api/v3/repos/{name}/pulls/{number}",gh_fixture::HOST),"repository_url":format!("https://{}/api/v3/repos/{name}",gh_fixture::HOST),"html_url":format!("https://{}/{name}/pull/{number}",gh_fixture::HOST),
        "user":actor,"head":{"sha":HEAD,"ref":"feature"},"base":{"sha":BASE,"ref":"main","repo":repository(name)},"requested_reviewers":[{"id":reviewer,"login":"octocat","type":"User"}],"requested_teams":[],"changed_files":0,"additions":0,"deletions":0,"mergeable":true,"mergeable_state":"clean"})
}
fn alpha() -> Value {
    alpha_with_clock(None)
}
fn alpha_with_clock(clock: Option<FixtureUtcAnchorV43>) -> Value {
    let created = clock.map(|clock| FixtureUtcAnchorV43::timestamp(clock.alpha_created()));
    let mut value = pull(
        APP,
        1,
        false,
        Some(created.as_deref().unwrap_or("2026-10-09T00:00:00Z")),
        json!({"id":51,"login":"alice","type":"User","avatar_url":null}),
        42,
    );
    if let Some(clock) = clock {
        clock.stamp_updated(&mut value);
    }
    value
}
fn rows(name: &str, clock: Option<FixtureUtcAnchorV43>) -> Vec<Value> {
    if name == APP {
        vec![alpha_with_clock(clock)]
    } else {
        let created = clock.map(|clock| FixtureUtcAnchorV43::timestamp(clock.bot_created()));
        let mut values = vec![
            pull(
                TOOLS,
                8,
                false,
                Some(created.as_deref().unwrap_or("2026-10-01T00:00:00Z")),
                json!({"id":52,"login":"metrics-bot","type":"Bot","avatar_url":null}),
                43,
            ),
            pull(TOOLS, 9, true, None, Value::Null, 42),
        ];
        if let Some(clock) = clock {
            for value in &mut values {
                clock.stamp_updated(value);
            }
        }
        values
    }
}
fn get(endpoint: &str, value: Value) -> GhStep {
    gh_fixture::read(endpoint, value)
}
fn inventory(name: &str, clock: Option<FixtureUtcAnchorV43>) -> Vec<GhStep> {
    vec![
        get(&format!("repos/{name}"), repository(name)),
        get(
            &format!(
                "repos/{name}/pulls?state=all&sort=created&direction=desc&per_page=100&page=1"
            ),
            json!(rows(name, clock)),
        ),
        get(&format!("repos/{name}"), repository(name)),
    ]
}
fn refresh(names: &[&str]) -> Vec<GhStep> {
    refresh_with_clock(names, None)
}
fn refresh_with_clock(names: &[&str], clock: Option<FixtureUtcAnchorV43>) -> Vec<GhStep> {
    let mut steps = gh_fixture::account(42);
    for name in names {
        steps.extend(inventory(name, clock));
    }
    steps.extend(gh_fixture::account(42));
    assert_eq!(steps.len(), 4 + 3 * names.len());
    steps
}
fn window(names: &[&str]) -> Vec<GhStep> {
    let mut steps = gh_fixture::account(42);
    // Root brackets cached recomputation with canonical access before and after,
    // without loading new inventories or replacing the immutable evidence clock.
    for name in names {
        steps.push(get(&format!("repos/{name}"), repository(name)));
    }
    for name in names {
        steps.push(get(&format!("repos/{name}"), repository(name)));
    }
    steps.extend(gh_fixture::account(42));
    assert_eq!(steps.len(), 4 + 2 * names.len());
    steps
}
fn canonical_identity(clock: Option<FixtureUtcAnchorV43>) -> Vec<GhStep> {
    vec![
        get(&format!("repos/{APP}"), repository(APP)),
        get(&format!("repos/{APP}/pulls/1"), alpha_with_clock(clock)),
        get(&format!("repos/{APP}"), repository(APP)),
    ]
}
fn details(clock: Option<FixtureUtcAnchorV43>) -> Vec<GhStep> {
    let mut steps = vec![get(
        &format!("repos/{APP}/pulls/1"),
        alpha_with_clock(clock),
    )];
    for suffix in [
        "issues/1/comments",
        "pulls/1/commits",
        "pulls/1/files",
        "pulls/1/reviews",
        "issues/1/timeline",
    ] {
        let mut step = get(&format!("repos/{APP}/{suffix}?per_page=100"), json!([[]]));
        step.args.extend(["--paginate".into(), "--slurp".into()]);
        steps.push(step);
    }
    steps.push(get(
        &format!("repos/{APP}/pulls/1"),
        alpha_with_clock(clock),
    ));
    assert_eq!(steps.len(), 7);
    steps
}
fn open_steps() -> Vec<GhStep> {
    open_steps_with_clock(None)
}
fn open_steps_with_clock(clock: Option<FixtureUtcAnchorV43>) -> Vec<GhStep> {
    let mut steps = gh_fixture::account(42);
    steps.extend(canonical_identity(clock));
    steps.extend(details(clock));
    steps.extend(canonical_identity(clock));
    steps.extend(gh_fixture::account(42));
    assert_eq!(steps.len(), 17);
    steps
}
fn journal(session: &mut Session, fixture: &Fixture, count: usize) {
    session.wait_for_command_count(fixture, count);
    let rows = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    assert!(rows
        .iter()
        .all(|row| row["args"][0] == "--version" || row["args"][5] == "GET"));
    assert!(
        !rows.iter().any(|row| row["args"][1]
            .as_str()
            .is_some_and(|endpoint| endpoint.starts_with("search/") || endpoint == "graphql")),
        "Repository-wide metrics do not use personal search or an inferred history query"
    );
}
fn metadata_only(fixture: &Fixture) {
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let scope = Scope::github(gh_fixture::HOST, Some(42)).unwrap();
    for record in cache.list(&scope, None).unwrap() {
        assert!(
            !matches!(
                record.kind.as_str(),
                "collaboration-document-v1" | "search-index-document-v1" | "metrics-event-v1"
            ),
            "Inventory/view navigation does not invent hydrated content or retained history"
        );
        assert!(
            record.payload.get("body").is_none(),
            "Dashboard persistence must not store the native PR body as metric evidence"
        );
    }
    assert!(cache
        .list(&Scope::github(gh_fixture::HOST, Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
}
fn preserve(fixture: &Fixture, head: &str, index: &[u8], status: &str) {
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"local dirty Dashboard bytes\n"
    );
}
fn start(session: &mut Session, mouse: bool) {
    session.wait_for_screen("Recent commits");
    choose(session, mouse, "5 Dashboard", b"5");
    session.wait_for_screen("Cohort: selected-repository PR inventory (all states)");
}
/// The Dashboard list's `▌` selected row contains `label`.
fn selected(session: &mut Session, label: &str) {
    session.wait_for_selected_row(&crate::support::view_title("Dashboard"), label);
}
/// A painted Cards table row: the METRIC cell `name`, then (after the column gap) the VALUE
/// cell that is exactly `value`.
pub(crate) fn has_card_value(screen: &str, name: &str, value: &str) -> bool {
    screen.lines().any(|line| {
        line.split_once(name).is_some_and(|(_, rest)| {
            rest.starts_with("  ")
                && rest.trim_start().strip_prefix(value).is_some_and(|tail| {
                    tail.is_empty() || tail.starts_with(' ') || tail.starts_with('│')
                })
        })
    })
}
pub(crate) fn wait_card_value(session: &mut Session, name: &str, value: &str) {
    let description = format!("card row {name} = {value}");
    if let Err(error) = session.try_wait_for_condition(
        |screen| has_card_value(screen, name, value),
        &description,
        Duration::from_secs(8),
    ) {
        session.save_artifacts();
        panic!("{error}; artifacts: {}", session.artifact_dir.display());
    }
}
fn select_card(session: &mut Session, index: usize, label: &str) {
    session.send_key(b"\x1b[H");
    selected(session, CARDS[0]);
    for label in CARDS.iter().take(index + 1).skip(1) {
        session.send_key(b"\x1b[B");
        selected(session, label);
    }
    selected(session, label);
}
fn card(session: &mut Session, mouse: bool, index: usize) {
    if mouse {
        session.click_visible_action(CARDS[index]);
    } else {
        select_card(session, index, CARDS[index]);
        session.send_key(b"\r");
    }
    session.wait_for_screen(CARDS[index]);
    session.wait_for_screen("Unknown");
    session.wait_for_screen("Censored");
}
fn back(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[b Back]", b"b");
}
fn scroll_to(session: &mut Session, mouse: bool, target: &str, anchor: &str) {
    for _ in 0..40 {
        if session
            .try_wait_for(target, Duration::from_millis(50))
            .is_ok()
        {
            return;
        }
        if mouse {
            let (column, row) = session.locate(anchor).unwrap_or((10, 16));
            session.wheel(column, row, true);
        } else {
            session.send_key(b"\x1b[B");
        }
    }
    session.wait_for_screen(target);
}
fn as_of(session: &mut Session) -> u64 {
    session.wait_for_screen("As-of ");
    let screen = session.screen();
    let line = screen.lines().find(|line| line.contains("As-of ")).unwrap();
    let start = line.find("As-of ").unwrap() + 6;
    let stamp = &line[start..start + 19];
    let part = |range: std::ops::Range<usize>| stamp[range].parse::<i32>().unwrap();
    // Independent libc UTC conversion; no production formatter/formula is used
    // to accept a window or numeric value to the application's current output.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = part(0..4) - 1900;
    tm.tm_mon = part(5..7) - 1;
    tm.tm_mday = part(8..10);
    tm.tm_hour = part(11..13);
    tm.tm_min = part(14..16);
    tm.tm_sec = part(17..19);
    u64::try_from(unsafe { libc::timegm(&mut tm) }).unwrap()
}
fn utc(seconds: u64) -> String {
    let seconds = libc::time_t::try_from(seconds).unwrap();
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    assert!(!unsafe { libc::gmtime_r(&seconds, &mut tm) }.is_null());
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}
fn assert_window(session: &mut Session, days: u64, clock: u64) {
    session.wait_for_screen(&format!(
        "UTC window [{}, {})",
        utc(clock - days * 86_400),
        utc(clock)
    ));
    session.wait_for_screen(&format!(
        "As-of {} UTC • snapshots are not trends",
        utc(clock)
    ));
}
fn cards_ready(session: &mut Session, open: u64, needs: u64) {
    session.wait_for_screen("METRIC");
    wait_card_value(session, "Loaded open PRs", &open.to_string());
    wait_card_value(session, "Needs my review", &needs.to_string());
    for label in &CARDS {
        session.wait_for_screen(label);
    }
}

#[test]
fn metrics_keyboard_repository_cohort_windows_all_cards_exact_native_open_and_return() {
    happy(false);
}
#[test]
fn metrics_mouse_repository_cohort_windows_all_cards_exact_native_open_and_return() {
    happy(true);
}
fn happy(mouse: bool) {
    let fixture_clock = FixtureUtcAnchorV43::capture();
    let fixture = configured(true);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let mut steps = refresh_with_clock(&[APP, TOOLS], Some(fixture_clock));
    steps.extend(window(&[APP, TOOLS]));
    steps.extend(window(&[APP, TOOLS]));
    steps.extend(refresh_with_clock(&[APP], Some(fixture_clock)));
    steps.extend(window(&[APP]));
    steps.extend(refresh_with_clock(&[APP, TOOLS], Some(fixture_clock)));
    steps.extend(refresh_with_clock(&[APP, TOOLS], Some(fixture_clock)));
    steps.extend(open_steps_with_clock(Some(fixture_clock)));
    assert_eq!(steps.len(), 76);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    let mut count = 10;
    journal(&mut session, &fixture, count);
    cards_ready(&mut session, 3, 1);
    session.wait_for_screen("Repositories: ALL 2 declared");
    session.wait_for_screen("Sources: PR Partial • Checks Unavailable • Actions Unavailable");
    let clock = as_of(&mut session);
    assert_window(&mut session, 7, clock);
    assert!(clock >= fixture_clock.0);
    let created_alpha = fixture_clock.alpha_created();
    let created_bot = fixture_clock.bot_created();
    let mean = (u128::from(clock - created_alpha) + u128::from(clock - created_bot)) / 2;
    wait_card_value(
        &mut session,
        "Mean open PR age",
        &format!("{mean}s mean / 2 measured"),
    );
    choose(&mut session, mouse, "[0 30 days]", b"0");
    count += 8;
    journal(&mut session, &fixture, count);
    cards_ready(&mut session, 3, 1);
    assert_eq!(as_of(&mut session), clock);
    assert_window(&mut session, 30, clock);
    choose(&mut session, mouse, "[7 7 days]", b"7");
    count += 8;
    journal(&mut session, &fixture, count);
    cards_ready(&mut session, 3, 1);
    assert_eq!(as_of(&mut session), clock);
    assert_window(&mut session, 7, clock);
    let commands = session.inspect_command_journal(&fixture);
    // Every card supports native contributor/formula details and Back. Unknown,
    // censored and unavailable labels are retained rather than converted to zero.
    for (index, formula) in FORMULAS.iter().enumerate() {
        card(&mut session, mouse, index);
        if index < 6 {
            choose(&mut session, mouse, "[i Details]", b"i");
            session.wait_for_screen("Repository ID: 410");
            session.wait_for_screen("PR ID: 1001");
            session.wait_for_screen("Genuine PR node: PR_dashboard_1");
            scroll_to(&mut session, mouse, "Actor source numeric ID: 51", TITLE);
            session.wait_for_screen("@alice");
            choose(&mut session, mouse, "[i Details]", b"i");
        } else {
            session.wait_for_screen("No contributing records");
            choose(&mut session, mouse, "[i Details]", b"i");
        }
        session.wait_for_screen("Unknown");
        session.wait_for_screen("Censored");
        if index >= 3 {
            session.wait_for_screen("Unavailable (not zero)");
        }
        scroll_to(&mut session, mouse, formula, "Unknown");
        back(&mut session, mouse);
        if index < 6 {
            back(&mut session, mouse);
        }
        back(&mut session, mouse);
        cards_ready(&mut session, 3, 1);
        journal(&mut session, &fixture, count);
        assert_eq!(session.inspect_command_journal(&fixture), commands);
    }
    card(&mut session, mouse, 2);
    session.resize(80, 24);
    session.wait_for_screen("Unknown 1");
    session.wait_for_screen("Censored 0");
    if mouse {
        let (x, y) = session.locate(TITLE).unwrap();
        session.wheel(x, y, true);
    } else {
        session.send_key(b"\x1b[F");
    }
    selected(&mut session, DRAFT);
    choose(&mut session, mouse, "[i Details]", b"i");
    scroll_to(
        &mut session,
        mouse,
        "Actor source numeric ID: Unavailable",
        DRAFT,
    );
    scroll_to(&mut session, mouse, "unknown actor [unknown]", DRAFT);
    session.wait_for_screen("[?]");
    scroll_to(&mut session, mouse, "Age unavailable", DRAFT);
    back(&mut session, mouse);
    session
        .try_wait_for_condition(
            |screen| {
                !screen.contains("Genuine PR node:")
                    && !screen.contains("[Enter Open")
                    && screen.contains(TITLE)
                    && screen.contains("Bounded bot housekeeping")
                    && screen.contains(DRAFT)
            },
            "age contributor list restored before the next Back click",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    selected(&mut session, DRAFT);
    back(&mut session, mouse);
    // SIGWINCH and mouse input are separate event streams. Prove Back reached
    // Cards at the old80-column geometry before changing the button's row.
    cards_ready(&mut session, 3, 1);
    session.resize(120, 40);
    cards_ready(&mut session, 3, 1);
    journal(&mut session, &fixture, count);
    assert_eq!(session.inspect_command_journal(&fixture), commands);
    choose(&mut session, mouse, "[R Repositories]", b"R");
    session.wait_for_screen("Choose ALL declared repositories or exactly one");
    if mouse {
        session.click_visible_action(&format!("[{APP}]"));
    } else {
        session.send_key(b"\x1b[B");
        selected(&mut session, &format!("[{APP}]"));
        session.send_key(b"\r");
    }
    count += 7;
    journal(&mut session, &fixture, count);
    cards_ready(&mut session, 1, 1);
    session.wait_for_screen(&format!("Repository: {APP}"));
    let one_clock = as_of(&mut session);
    assert_window(&mut session, 7, one_clock);
    choose(&mut session, mouse, "[0 30 days]", b"0");
    count += 6;
    journal(&mut session, &fixture, count);
    cards_ready(&mut session, 1, 1);
    assert_eq!(as_of(&mut session), one_clock);
    assert_window(&mut session, 30, one_clock);
    choose(&mut session, mouse, "[R Repositories]", b"R");
    session.wait_for_screen("[All 2 declared repositories]");
    if mouse {
        session.click_visible_action("[All 2 declared repositories]");
    } else {
        session.send_key(b"\x1b[H");
        selected(&mut session, "[All 2 declared repositories]");
        session.send_key(b"\r");
    }
    count += 10;
    journal(&mut session, &fixture, count);
    cards_ready(&mut session, 3, 1);
    session.wait_for_screen("Repositories: ALL 2 declared");
    let all_clock = as_of(&mut session);
    assert_window(&mut session, 30, all_clock);
    choose(&mut session, mouse, "[r Refresh]", b"r");
    count += 10;
    journal(&mut session, &fixture, count);
    cards_ready(&mut session, 3, 1);
    let fresh_clock = as_of(&mut session);
    assert_window(&mut session, 30, fresh_clock);
    card(&mut session, mouse, 1);
    session.wait_for_screen(TITLE);
    // Contributor rows are table rows: ITEM, TITLE, then the ACTOR cell (the filled two-cell
    // initials chip, exactly one blank cell and the login without `@`).
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("ITEM")
                    && screen.lines().any(|line| {
                        line.contains(APP)
                            && line.contains(TITLE)
                            && (line.contains("AL alice") || line.contains("[AL] alice"))
                    })
            },
            "contributor table row with the alice chip",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    let before_window = session
        .screen()
        .lines()
        .find(|line| line.contains("UTC window ["))
        .unwrap()
        .to_owned();
    if mouse {
        session.click_visible_action(TITLE);
    } else {
        session.send_key(b"\r");
    }
    count += 17;
    journal(&mut session, &fixture, count);
    session.wait_for_screen("Inbox • focused view");
    session.wait_for_screen(BODY);
    session.wait_for_screen(HEAD);
    assert!(!session.screen().contains("Loaded open PRs"));
    let open_commands = session.inspect_command_journal(&fixture);
    // Native Back follows the exact Dashboard origin, not a new tab/load. The
    // retained contributor selection, scope and immutable window survive it.
    choose(&mut session, mouse, "[b Back]", b"b");
    session.wait_for_screen("Dashboard • focused view");
    session.wait_for_screen("Needs my review");
    session.wait_for_screen(TITLE);
    selected(&mut session, TITLE);
    session.wait_for_screen("Repositories: ALL 2 declared");
    assert_window(&mut session, 30, fresh_clock);
    session.wait_for_screen(before_window.trim());
    journal(&mut session, &fixture, count);
    assert_eq!(session.inspect_command_journal(&fixture), open_commands);
    assert_eq!(count, steps.len());
    metadata_only(&fixture);
    preserve(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn metrics_keyboard_cached_window_access_loss_hides_values_breadcrumb_and_stale_open() {
    access_loss(false);
}
#[test]
fn metrics_mouse_cached_window_access_loss_hides_values_breadcrumb_and_stale_open() {
    access_loss(true);
}
fn access_loss(mouse: bool) {
    let fixture = configured(false);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let mut steps = refresh(&[APP]);
    steps.extend(gh_fixture::account(42));
    let mut denied = get(&format!("repos/{APP}"), Value::Null);
    denied.status = 1;
    denied.stdout.clear();
    denied.stderr = b"gh: Selected repository inaccessible (HTTP 403)\n".to_vec();
    denied.delay_ms = 250;
    steps.push(denied);
    steps.extend(gh_fixture::account(42));
    assert_eq!(steps.len(), 12);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    journal(&mut session, &fixture, 7);
    cards_ready(&mut session, 1, 1);
    card(&mut session, mouse, 0);
    session.wait_for_screen(TITLE);
    let stale_open = session.locate(TITLE).unwrap();
    choose(&mut session, mouse, "[0 30 days]", b"0");
    session.wait_for_screen("prior values hidden");
    assert!(!session.screen().contains(TITLE));
    assert!(!has_card_value(&session.screen(), "Loaded open PRs", "1"));
    journal(&mut session, &fixture, 12);
    session.wait_for_screen("Dashboard access changed • previous values hidden");
    assert!(!session.screen().contains(TITLE));
    assert!(!session.screen().contains("example/app#1"));
    assert!(!has_card_value(&session.screen(), "Loaded open PRs", "1"));
    assert!(!session.screen().contains("Repository: example/app"));
    assert!(!session.screen().contains("Cohort: selected-repository"));
    session.resize(80, 24);
    session.wait_for_screen("Dashboard access changed • previous values hidden");
    let commands = session.inspect_command_journal(&fixture);
    if mouse {
        session.click(stale_open.0.min(70), stale_open.1.min(18));
    } else {
        session.send_key(b"\r");
    }
    // Actual Tab/focus rendering proves the input stream advanced beyond the
    // stale Open event; the final post-exit journal also covers late workers.
    session.focus_footer_control("[q Quit]");
    journal(&mut session, &fixture, 12);
    assert_eq!(session.inspect_command_journal(&fixture),commands,"A stale contributor cannot reopen/refetch after access loss; only explicit Refresh may recover");
    assert!(!session.screen().contains(BODY));
    assert!(!session.screen().contains(TITLE));
    metadata_only(&fixture);
    preserve(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    journal(&mut session, &fixture, 12);
}
