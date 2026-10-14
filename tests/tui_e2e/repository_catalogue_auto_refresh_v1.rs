//! SOURCE-ONLY Task9 delayed discovery / expired legacy timer regression.
//! Root registers this as a catalogue native child. Two real-binary journeys
//! remain UNRUN until its gate; controlled fake-gh latency is the test input,
//! never a sleep-based assumption that zero provider requests occurred.
use super::*;

const PAGE_DELAY_MS: u64 = 6000;
const REFRESH_SECONDS: u64 = 5;
const HELP_TITLE: &str = "Help • keyboard and mouse";

fn open_first_my_prs() -> Vec<GhStep> {
    let mut result = principal(42);
    result.push(get(&format!("repos/{FIRST}"), repository(0)));
    // The exact-scoped legacy workspace still has its own positive identity
    // reads. Do not fabricate a source receipt or subtract these actual calls.
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
                &format!("q=is:pr is:open {qualifier}:octocat repo:{FIRST}"),
                "-f",
                "per_page=100",
                "-f",
                "page=1",
            ],
            "{\"total_count\":0,\"incomplete_results\":false,\"items\":[]}",
        ));
    }
    result.push(get(&format!("repos/{FIRST}"), repository(0)));
    result.extend(principal(42));
    assert_eq!(
        result.len(),
        11,
        "Explicit original target, inner5, closing original target and principal"
    );
    result
}

fn delayed_protocol() -> Vec<GhStep> {
    let mut source = collection(vec![json!([repository(0)])], 42);
    assert_eq!(source.len(), 7);
    assert_eq!(source[3].args[1], endpoint(1));
    source[3].delay_ms = PAGE_DELAY_MS;
    source.extend(open_first_my_prs());
    assert_eq!(source.len(), 18);
    source
}

fn assert_exact_commands(session: &Session, fixture: &Fixture) {
    assert_commands(session, fixture, 18);
    let rows = commands(session, fixture);
    let expected = delayed_protocol();
    for (ordinal, (actual, expected)) in rows.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual["args"],
            json!(expected.args),
            "Exact child{ordinal}, not default/all/timer refresh"
        );
    }
    let queries = rows
        .iter()
        .filter(|row| row["args"][1] == "search/issues")
        .collect::<Vec<_>>();
    assert_eq!(
        queries.len(),
        3,
        "One explicit refresh, not an automatic legacy resend"
    );
    assert!(queries.iter().all(|row| row["args"][7]
        .as_str()
        .unwrap()
        .ends_with(&format!("repo:{FIRST}"))));
}

fn assert_original_intents(fixture: &Fixture) {
    let cache = Cache::open(&database(fixture)).unwrap();
    let original = Scope::github(HOST, Some(42)).unwrap();
    assert_eq!(
        cache.draft(&original, "catalogue-draft").unwrap(),
        Some("Original42 draft".into())
    );
    assert!(
        cache.saved_views(&original).unwrap().is_empty(),
        "Discovery/Open must not auto-Save or activate a durable set"
    );
    let other = Scope::github(HOST, Some(43)).unwrap();
    assert!(cache.saved_views(&other).unwrap().is_empty());
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.draft(&other, "catalogue-draft").unwrap().is_none());
}

fn fresh_original_inbox(session: &mut Session) {
    session
        .try_wait_for_condition(
            |screen| {
                [
                    "Inbox • focused view",
                    HOST,
                    "@octocat",
                    FIRST,
                    "My PRs",
                    "Fresh • fetched",
                    "0 unique PRs across 0 queue matches • complete",
                    "No pull requests in this queue / scope.",
                ]
                .iter()
                .all(|label| screen.contains(label))
                    && !screen.contains("Refreshing")
                    && !screen.contains("Current ID-bound repository access required")
                    && !screen.contains("all repositories on host")
                    && !screen.contains("legacy fallback not used")
            },
            "Fresh explicit FIRST inbox retained after expired timer and processed input",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
}

fn delayed_discovery(mouse: bool) {
    let fixture = configured(&delayed_protocol());
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let mut config = Config::load(&config_path).unwrap();
    // This is the actual minimum accepted by Config::validate, not zero to
    // invent an unsupported timer state. Test setup changes config before run.
    config.refresh_seconds = REFRESH_SECONDS;
    config.save(&config_path).unwrap();
    let config_before = fs::read(&config_path).unwrap();
    let git_before = git_state(&fixture);
    assert_original_intents(&fixture);
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    let initial_git = git_commands(&fixture);
    let source_started = Instant::now();
    discover(&mut session, mouse);
    // The shared strict fixture journals BEFORE its controlled child delay.
    // Observe the real busy model and no private names/counts before final7.
    session.wait_for_command_count(&fixture, 4);
    ready(
        &mut session,
        &[
            TITLE,
            "Original worker pending",
            "private rows/counts hidden",
        ],
    );
    assert!(
        !session.screen().contains(FIRST),
        "No repository row before original final source proof"
    );
    assert!(!session.screen().contains("Observed: 1"));
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        TITLE,
        &["[q Quit]", "[Esc Close]"],
    );
    session.wait_for_command_count(&fixture, 7);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 1",
            FIRST,
            "Repository ID: 10000",
            "API-visible membership complete",
        ],
    );
    assert!(
        source_started.elapsed() >= Duration::from_millis(PAGE_DELAY_MS),
        "Real child delay must finish, not skipped fixture readiness"
    );
    assert!(
        source_started.elapsed() > Duration::from_secs(REFRESH_SECONDS),
        "The app's original timer began BEFORE this genuine6s traversal"
    );
    assert_commands(&session, &fixture, 7);
    assert_eq!(
        git_commands(&fixture),
        initial_git,
        "Delayed remote discovery has no local refresh/clone effects"
    );
    assert_original_intents(&fixture);
    filter(
        &mut session,
        mouse,
        "repo0000",
        FIRST,
        "Repository ID: 10000",
    );
    choose(&mut session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_command_count(&fixture, 18);
    session.wait_for_overlay_close(TITLE);
    fresh_original_inbox(&mut session);
    assert_exact_commands(&session, &fixture);
    assert_original_intents(&fixture);
    // The intended fresh exact refresh may write disposable normalized cache
    // rows. Pure event barriers must not rewrite them or mutate durable intent.
    let original_scope = Scope::github(HOST, Some(42)).unwrap();
    let original_records = Cache::open(&database(&fixture))
        .unwrap()
        .list(&original_scope, None)
        .unwrap();
    assert!(
        !original_records.is_empty(),
        "Independent actual cache publication evidence"
    );
    let commands_before = session.inspect_command_journal(&fixture);
    let git_trace_before = git_commands(&fixture);

    // Every transition follows actual processed input/new-frame geometry, not
    // a blind pause. The elapsed legacy timer gets many event-loop opportunities
    // to erase this page if it still schedules a Some(scope) legacy refresh.
    session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
    fresh_original_inbox(&mut session);
    if mouse {
        session.click_visible_action("[? Help]");
    } else {
        session.focus_visible_action("[? Help]");
        session.send_key(b"\r");
    }
    overlay_ready(
        &mut session,
        HELP_TITLE,
        &["Inbox • help for this view", "[Esc Close]"],
    );
    if mouse {
        session.close_overlay_by_mouse(HELP_TITLE);
    } else {
        session.close_overlay_by_key(HELP_TITLE);
    }
    fresh_original_inbox(&mut session);
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    fresh_original_inbox(&mut session);
    assert_eq!(
        session.inspect_command_journal(&fixture),
        commands_before,
        "Expired timer cannot launch any default or automatic legacy read"
    );
    assert_eq!(
        git_commands(&fixture),
        git_trace_before,
        "Fresh view/Help/focus/resize remains local read-only"
    );
    assert_eq!(
        Cache::open(&database(&fixture))
            .unwrap()
            .list(&original_scope, None)
            .unwrap(),
        original_records,
        "Event barriers cannot rewrite freshly admitted scoped cache rows"
    );
    assert_original_intents(&fixture);
    assert_exact_commands(&session, &fixture);
    finish(
        &mut session,
        &fixture,
        mouse,
        18,
        &git_before,
        &config_before,
    );
    assert_original_intents(&fixture);
    assert_eq!(
        Cache::open(&database(&fixture))
            .unwrap()
            .list(&original_scope, None)
            .unwrap(),
        original_records
    );
}

#[test]
fn repo_catalog_auto_refresh_keyboard_delayed_source_retains_original_fresh_inbox() {
    delayed_discovery(false);
}
#[test]
fn repo_catalog_auto_refresh_mouse_delayed_source_retains_original_fresh_inbox() {
    delayed_discovery(true);
}
