//! SOURCE-ONLY Task9 explicit empty/unresolved set boundaries. Register as a
//! child of repository_catalogue_native_v1. Four real subprocess journeys are
//! UNRUN until Root's native gate; persisted JSON below is an independent
//! intent oracle, NEVER a source/permission receipt or direct handler shortcut.
use super::*;

#[derive(Clone, Copy)]
enum Stop {
    Empty,
    Unresolved,
}
impl Stop {
    fn prefix(self) -> &'static str {
        match self {
            Self::Empty => "Explicit repository scope is empty",
            Self::Unresolved => "Original repository scope is unresolved",
        }
    }
    fn modal_fragment(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Unresolved => "unresolved",
        }
    }
    fn explanation(self) -> &'static str {
        match self {
            Self::Empty => "Explicit repository scope is empty • no fallback or query sent",
            Self::Unresolved => {
                "Original repository scope is unresolved • no narrowing, fallback or query sent"
            }
        }
    }
}

fn visible_words(screen: &str) -> String {
    // The ordinary prepared paragraph wraps at narrow widths. Normalize only
    // terminal whitespace and observed box-edge cells; do not read App/context
    // state or weaken the exact semantic explanation to a status substring.
    screen
        .lines()
        .map(|line| line.trim().trim_matches('│').trim())
        .collect::<Vec<_>>()
        .join(" ")
}

fn unresolved_activation() -> Vec<GhStep> {
    let mut steps = principal(42);
    let mut denied = get(&format!("repos/{FIRST}"), Value::Null);
    denied.status = 1;
    denied.stdout.clear();
    denied.stderr = b"gh: repository not found (HTTP 404)\n".to_vec();
    steps.push(denied);
    steps.extend(principal(42));
    assert_eq!(
        steps.len(),
        5,
        "Original ghost reference checked; no query/body/default repo"
    );
    steps
}

fn saved_original(fixture: &Fixture, unresolved: bool) -> Value {
    let expected = if unresolved { vec![10000] } else { vec![] };
    let value = assert_intent(fixture, &expected);
    if unresolved {
        assert_eq!(value["pins"]["10000"]["id"], 10000);
        assert_eq!(value["pins"]["10000"]["node_id"], "REPO_catalogue_10000");
        assert_eq!(value["pins"]["10000"]["last_name"], FIRST);
    } else {
        assert!(
            value["pins"].as_object().unwrap().is_empty(),
            "No unselected pins invented"
        );
    }
    value
}

fn stopped_frame(session: &mut Session, view: &str, stop: Stop) {
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains(&format!("{view} • focused view"))
                    && screen.contains(stop.prefix())
                    && visible_words(screen).contains(stop.explanation())
                    && !screen.contains("Refreshing")
                    && !screen.contains("Loading")
            },
            &format!("Actual {view} stopped-scope explanation after input"),
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    let screen = session.screen();
    assert!(
        !screen.contains("0 unique PRs"),
        "No falsely successful empty personal inbox"
    );
    assert!(
        !screen.contains("0 runs of 0"),
        "No falsely successful empty Actions inventory"
    );
    assert!(
        !screen.contains("No pull requests in this queue"),
        "Unavailable scope is not complete empty PR data"
    );
    assert!(
        !screen.contains("0 contributors"),
        "Missing cohort is not a zero metric"
    );
    assert!(
        !screen.contains("Current ID-bound repository access required"),
        "Explicit Empty/Unresolved remains tagged, not nonempty or default"
    );
    assert!(
        !screen.contains("private repositories hidden"),
        "Merely empty/unresolved intent does not claim an account/privacy revocation"
    );
}

fn activate_stopped(session: &mut Session, mouse: bool, stop: Stop, observed: usize) {
    activate(session, mouse);
    ready(
        session,
        &[
            TITLE,
            &format!("Observed: {observed}"),
            "API-visible membership complete",
        ],
    );
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("Active explicit set")
                    && screen.contains(SET)
                    && screen.contains(stop.modal_fragment())
                    && !screen.contains("current access observed")
                    && !screen.contains("Original worker pending")
            },
            "Honest explicit Empty/Unresolved activation status, not a current access grant",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
}

fn stopped_views(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    stop: Stop,
    count: usize,
    original: &Value,
    unresolved: bool,
) {
    let gh = session.inspect_command_journal(fixture);
    let git = git_commands(fixture);
    if unresolved {
        // Original Retry now retains missing local choices, so Close must
        // exercise Back and deliberate local discard instead of losing them.
        super::read_limits_v1::discard_close(session, mouse);
    } else {
        choose(session, mouse, "[Esc Close]", b"\x1b");
        session.wait_for_overlay_close(TITLE);
        session.wait_for_main_frame();
    }
    ready(session, &["Repository • focused view", "Recent commits"]);
    assert_eq!(session.inspect_command_journal(fixture), gh);
    assert_eq!(git_commands(fixture), git);
    assert_eq!(&saved_original(fixture, unresolved), original);
    // Every view uses an ACTUAL key or observed tab click. Refresh must remain
    // bounded STOP even though its legacy empty Vec would normally mean all.
    // Actual SIGWINCH/redraw gives an event/frame barrier after each Refresh;
    // do not infer zero requests from an arbitrary sleep or unchanged snapshot.
    for (index, (name, tab, key)) in [
        ("Inbox", "1 Inbox", b"1".as_slice()),
        ("Actions", "4 Actions", b"4".as_slice()),
        ("Search", "3 Search", b"3".as_slice()),
        ("Dashboard", "5 Dashboard", b"5".as_slice()),
    ]
    .into_iter()
    .enumerate()
    {
        choose(session, mouse, tab, key);
        stopped_frame(session, name, stop);
        choose(session, mouse, "[r Refresh]", b"r");
        let wide = mouse == (index % 2 == 0);
        session.resize(if wide { 120 } else { 80 }, if wide { 40 } else { 24 });
        stopped_frame(session, name, stop);
        assert_eq!(
            session.inspect_command_journal(fixture),
            gh,
            "{name} navigation/Refresh must not issue ANY principal/default queries"
        );
        assert_commands(session, fixture, count);
        assert_eq!(
            git_commands(fixture),
            git,
            "Stopped {name} cannot clone/fetch/open a mapped local checkout"
        );
        assert_eq!(
            &saved_original(fixture, unresolved),
            original,
            "View/Refresh cannot delete, narrow, or rewrite saved original IDs/nodes"
        );
    }
    // Local browsing remains useful without releasing the blocked remote tag.
    choose(session, mouse, "2 Repository", b"2");
    ready(session, &["Repository • focused view", "Recent commits"]);
    assert_eq!(session.inspect_command_journal(fixture), gh);
    assert_eq!(
        git_commands(fixture),
        git,
        "Returning to already prepared local Repository is pure"
    );
}

fn empty_set(mouse: bool) {
    let mut steps = collection(vec![json!([repository(0)])], 42);
    steps.extend(save_intent());
    assert_eq!(
        steps.len(),
        11,
        "Genuine inventory7, explicit empty Save4, Activate noAPI"
    );
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    session.wait_for_command_count(&fixture, 7);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 1",
            FIRST,
            "Repository ID: 10000",
            "Selected: no",
            "Pinned: no",
        ],
    );
    assert_commands(&session, &fixture, 7);
    // Source inventory membership1 does not select a repository implicitly.
    // Saving an empty set is an explicit metadata intent, not global/default.
    filter(
        &mut session,
        mouse,
        "repo0000",
        FIRST,
        "Repository ID: 10000",
    );
    ready(&mut session, &["Selected: no", "Pinned: no"]);
    save(&mut session, mouse);
    assert_commands(&session, &fixture, 11);
    let original = saved_original(&fixture, false);
    activate_stopped(&mut session, mouse, Stop::Empty, 1);
    assert_commands(&session, &fixture, 11);
    assert_eq!(saved_original(&fixture, false), original);
    stopped_views(
        &mut session,
        &fixture,
        mouse,
        Stop::Empty,
        11,
        &original,
        false,
    );
    finish(&mut session, &fixture, mouse, 11, &before, &config);
    assert_eq!(saved_original(&fixture, false), original);
}

fn unresolved_set(mouse: bool) {
    let mut steps = collection(vec![json!([repository(0)])], 42);
    steps.extend(save_intent());
    // Actual exhausted, same-principal/owner empty enumeration is not deletion
    // authority for the saved original set or pin.
    steps.extend(collection(vec![json!([])], 42));
    steps.extend(unresolved_activation());
    assert_eq!(
        steps.len(),
        23,
        "Source7+Save4+explicit Retry7+original404 access5"
    );
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    ready(
        &mut session,
        &[TITLE, "Observed: 1", FIRST, "Repository ID: 10000"],
    );
    assert_commands(&session, &fixture, 7);
    filter(
        &mut session,
        mouse,
        "repo0000",
        FIRST,
        "Repository ID: 10000",
    );
    choose(&mut session, mouse, "[Space Select]", b" ");
    choose(&mut session, mouse, "[p Pin]", b"p");
    ready(&mut session, &[FIRST, "Selected: yes", "Pinned: yes"]);
    save(&mut session, mouse);
    assert_commands(&session, &fixture, 11);
    let original = saved_original(&fixture, true);
    choose(&mut session, mouse, "[r Retry]", b"r");
    session.wait_for_command_count(&fixture, 18);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 0",
            "API-visible membership complete",
            "No matching repositories",
        ],
    );
    assert_commands(&session, &fixture, 18);
    assert_eq!(
        saved_original(&fixture, true),
        original,
        "Actual terminal empty traversal retains genuine saved ghost/pin"
    );
    // A new source workspace starts with its Filter focused. Leave it before
    // the keyboard activation shortcut; do not type 'a' into an empty filter.
    if !mouse {
        session.send_key(b"\t");
    }
    activate_stopped(&mut session, mouse, Stop::Unresolved, 0);
    session.wait_for_command_count(&fixture, 23);
    assert_commands(&session, &fixture, 23);
    let actual = commands(&session, &fixture);
    assert_eq!(
        actual[20]["args"],
        json!([
            "api",
            format!("repos/{FIRST}"),
            "--hostname",
            HOST,
            "--method",
            "GET"
        ])
    );
    assert_eq!(
        saved_original(&fixture, true),
        original,
        "404 access hides active membership, not durable original intent"
    );
    stopped_views(
        &mut session,
        &fixture,
        mouse,
        Stop::Unresolved,
        23,
        &original,
        true,
    );
    finish(&mut session, &fixture, mouse, 23, &before, &config);
    assert_eq!(saved_original(&fixture, true), original);
}

#[test]
fn repo_catalog_scope_stop_keyboard_explicit_saved_empty_never_defaults_any_remote_view() {
    empty_set(false);
}
#[test]
fn repo_catalog_scope_stop_mouse_explicit_saved_empty_never_defaults_any_remote_view() {
    empty_set(true);
}
#[test]
fn repo_catalog_scope_stop_keyboard_saved_original_404_unresolved_never_narrows_or_defaults() {
    unresolved_set(false);
}
#[test]
fn repo_catalog_scope_stop_mouse_saved_original_404_unresolved_never_narrows_or_defaults() {
    unresolved_set(true);
}
