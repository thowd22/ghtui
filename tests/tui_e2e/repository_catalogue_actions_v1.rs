//! SOURCE-ONLY native Task9 Actions routes. Root registers this as a child of
//! repository_catalogue_native_v1; both real-binary journeys are UNRUN until its
//! exclusive Cargo/native gate. Original source metadata is never a permission
//! receipt, and the existing one-child Actions pagination is explicitly counted.
use super::*;

const CURRENT: &str = "example/ci";
const INTENT_CANARY: &str = "catalogue-actions-observed-intent";

fn current_repository() -> Value {
    let mut row = repository(0);
    // Actual same-ID/node/owner leaf rename, never a fabricated new Read. Both
    // verification GETs retain the original FIRST locator throughout the route.
    row["full_name"] = json!(CURRENT);
    row
}

fn run(id: u64, name: &str, actor: &str, actor_kind: &str) -> Value {
    json!({
        "id":id,"workflow_id":7,"name":name,"display_title":"Discovery CI",
        "head_sha":"a".repeat(40),"head_branch":"main","event":"push",
        "status":"completed","conclusion":"failure","run_attempt":2,
        "created_at":format!("2026-10-13T12:00:{:02}Z",125-id),
        "actor":{"id":800+id,"login":actor,"type":actor_kind},
        "triggering_actor":{"id":42,"login":"octocat","type":"User"}
    })
}

fn actions_protocol() -> Vec<GhStep> {
    let mut result = collection(vec![json!([repository(0)])], 42);
    assert_eq!(result.len(), 7);
    result.extend(principal(42));
    result.push(get(&format!("repos/{FIRST}"), current_repository()));
    let mut catalog = get(
        &format!("repos/{CURRENT}/actions/runs?per_page=100"),
        json!([{"total_count":2,"workflow_runs":[
            run(123,"CI","alice","User"),run(124,"LINT","ci-bot","Bot")
        ]}]),
    );
    // This is exactly the existing run_catalog adapter's one bounded child.
    // Fixed-page repository enumeration MUST NOT acquire these legacy flags.
    catalog.args.extend(["--paginate".into(), "--slurp".into()]);
    result.push(catalog);
    result.push(get(&format!("repos/{FIRST}"), current_repository()));
    result.extend(principal(42));
    assert_eq!(result.len(), 14);
    result
}

fn assert_actions_commands(session: &Session, fixture: &Fixture) {
    let actual = commands(session, fixture);
    let expected = actions_protocol();
    assert_eq!(
        actual.len(),
        expected.len(),
        "No default or second automatic load"
    );
    for (ordinal, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual["args"],
            json!(expected.args),
            "Actual child {ordinal}"
        );
        assert_eq!(actual["matched"], true, "Exact offline child {ordinal}");
        assert_eq!(actual["stdin_hex"], "", "Read-only child {ordinal}");
    }
    assert_eq!(
        actual
            .iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "--paginate"))
            .count(),
        1,
        "Only the explicitly budgeted legacy run_catalog child paginates"
    );
    assert!(!actual.iter().any(
        |row| row["args"].as_array().unwrap().iter().any(|arg| matches!(
            arg.as_str(),
            Some("POST" | "PATCH" | "PUT" | "DELETE" | "graphql")
        ))
    ));
}

fn original_intent() -> Value {
    // An unrelated existing account-scoped navigation intent canary. It has no
    // source receipt, is not a catalogue approval/set, and must NOT auto-activate.
    json!({"kind":"observed-navigation-intent","original_uid":42,
        "unavailable_repository_id":90001,"last_name":"unavailable/old-ci"})
}

fn assert_private_intents_unchanged(fixture: &Fixture) {
    let cache = Cache::open(&database(fixture)).unwrap();
    let original = Scope::github(HOST, Some(42)).unwrap();
    assert_eq!(
        cache.draft(&original, "catalogue-draft").unwrap(),
        Some("Original42 draft".into())
    );
    assert_eq!(
        cache.saved_views(&original).unwrap(),
        vec![(INTENT_CANARY.into(), original_intent())]
    );
    let other = Scope::github(HOST, Some(43)).unwrap();
    assert!(cache.saved_views(&other).unwrap().is_empty());
    assert!(cache.draft(&other, "catalogue-draft").unwrap().is_none());
}

fn discover_with_captured_actions_tab(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    underlying_actions_tab: (u16, u16),
) {
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
    // A genuine stale UNDERLYING tab input is captured by the source overlay,
    // not a direct dispatcher assertion. Keyboard4 is not an accepted source
    // shortcut; a mouse event uses the actually observed original tab geometry.
    if mouse {
        session.click(underlying_actions_tab.0, underlying_actions_tab.1);
    } else {
        session.send_key(b"4");
    }
    overlay_ready(session, SOURCE, &["Owner", "[Enter Discover]"]);
    assert_commands(session, fixture, 0);
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
    session.wait_for_command_count(fixture, 7);
    ready(
        session,
        &[
            TITLE,
            FIRST,
            "Repository ID: 10000",
            "Selected: no",
            "Pinned: no",
        ],
    );
    assert_commands(session, fixture, 7);
}

fn actions_ready(session: &mut Session) {
    ready(
        session,
        &[
            "Actions • focused view",
            CURRENT,
            "2 runs of 2 reported • complete",
            "@alice",
            "@ci-bot [bot]",
            "run 123 attempt 2",
            "run 124 attempt 2",
            "failure",
        ],
    );
    assert!(
        !session.screen().contains(TITLE),
        "Destination dismisses the discovery overlay"
    );
}

fn wait_selected_actor(session: &mut Session, actor: &str, run_id: &str) {
    session
        .try_wait_for_condition(
            |screen| {
                screen.lines().any(|line| {
                    line.contains(actor) && line.contains(run_id) && line.contains("> ")
                })
            },
            &format!("Actual selected Actions row {actor} / {run_id}"),
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
}

fn discovery_to_actions(mouse: bool) {
    let fixture = configured(&actions_protocol());
    {
        let cache = Cache::open(&database(&fixture)).unwrap();
        cache
            .save_view(
                &Scope::github(HOST, Some(42)).unwrap(),
                INTENT_CANARY,
                &original_intent(),
            )
            .unwrap();
    }
    let git_before = git_state(&fixture);
    let config_before = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    assert_private_intents_unchanged(&fixture);
    let mut session = launch(&fixture, mouse);
    let tab = session
        .locate("4 Actions")
        .expect("Actual underlying global Actions tab");
    open_catalogue(&mut session, &fixture, mouse, 0);
    let local_trace = git_commands(&fixture);
    discover_with_captured_actions_tab(&mut session, &fixture, mouse, tab);
    assert_eq!(
        git_commands(&fixture),
        local_trace,
        "Remote discovery must not load or clone local Git"
    );
    assert_private_intents_unchanged(&fixture);

    // Explicitly focus the observed original row before the action. A capital A
    // in a focused Filter is text, not a keyboard route; Tab gives list focus.
    filter(
        &mut session,
        mouse,
        "repo0000",
        FIRST,
        "Repository ID: 10000",
    );
    // The catalogue owns its full terminal/footer, so its resize oracle must
    // observe actual modal redraw/geometry, not the ordinary Help/Quit footer.
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        TITLE,
        &["[q Quit]", "[Esc Close]"],
    );
    ready(&mut session, &[TITLE, FIRST, "Repository ID: 10000"]);
    assert_commands(&session, &fixture, 7);
    choose(&mut session, mouse, "[Open Actions]", b"A");
    session.wait_for_command_count(&fixture, 14);
    session.wait_for_overlay_close(TITLE);
    actions_ready(&mut session);
    assert_actions_commands(&session, &fixture);
    assert_private_intents_unchanged(&fixture);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config_before
    );

    // Run actors, failure labels and original repository identity remain usable
    // at both widths. Pure row selection/hover wheel does NOT open a run or
    // invoke implicit hydration. A left click on a run would intentionally open
    // it, so the mouse journey uses the genuine hovered-list wheel instead.
    let journal_before = session.inspect_command_journal(&fixture);
    let trace_before = git_commands(&fixture);
    if mouse {
        let (column, row) = session.locate("run 123 attempt 2").unwrap();
        session.wheel(column, row, true);
    } else {
        session.send_key(b"\x1b[B");
    }
    wait_selected_actor(&mut session, "@ci-bot [bot]", "run 124");
    // Actions has dismissed the modal and restores the ordinary footer.
    session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
    actions_ready(&mut session);
    wait_selected_actor(&mut session, "@ci-bot [bot]", "run 124");
    if mouse {
        let (column, row) = session.locate("run 124 attempt 2").unwrap();
        session.wheel(column, row, false);
    } else {
        session.send_key(b"\x1b[H");
    }
    wait_selected_actor(&mut session, "@alice", "run 123");
    assert_eq!(
        session.inspect_command_journal(&fixture),
        journal_before,
        "Pure input cannot refresh/expand the selected scope"
    );
    assert_eq!(
        git_commands(&fixture),
        trace_before,
        "No Git effects from prepared Actions selection/resize"
    );
    assert_actions_commands(&session, &fixture);
    assert_private_intents_unchanged(&fixture);
    assert!(
        !session.screen().contains("unavailable/old-ci"),
        "Unrelated saved intent cannot become active scope"
    );
    assert!(
        !session.screen().contains(FIRST),
        "Current Actions context uses separately verified canonical name"
    );
    assert_read_only_git(&fixture);
    assert_eq!(
        git_state(&fixture),
        git_before,
        "Read-only browsing preserves actual refs/index/staged/dirty/untracked bytes"
    );
    session.quit_and_assert_restored_terminal(mouse);
    assert_actions_commands(&session, &fixture);
    assert_private_intents_unchanged(&fixture);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config_before
    );
    assert_eq!(git_state(&fixture), git_before);
    assert_read_only_git(&fixture);
}

#[test]
fn repo_catalog_actions_keyboard_discover_original_id_to_current_canonical_actions_readonly() {
    discovery_to_actions(false);
}

#[test]
fn repo_catalog_actions_mouse_discover_original_id_to_current_canonical_actions_readonly() {
    discovery_to_actions(true);
}
