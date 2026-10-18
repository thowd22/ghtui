//! Both-input real-binary giant-actor boundary. Child of
//! exact_avatar_v1 reuses genuine raw fixtures/oracles, not direct App handlers.
use super::*;
fn journey(mouse: bool) {
    let fixture = fixture();
    fs::write(
        fixture.repo().join("avatar-untracked.txt"),
        b"Keep my untracked bytes\n",
    )
    .unwrap();
    let mut steps = protocol(Mode::Ready);
    for step in steps.iter_mut().skip(4) {
        let mut response: Value = serde_json::from_slice(&step.stdout).unwrap();
        response["items"][0]["user"]["name"] = json!("x".repeat(4097));
        step.stdout = serde_json::to_vec(&response).unwrap();
    }
    fixture.scripted_gh(&steps);
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let config = Config {
        avatars: true,
        refresh_seconds: 3600,
        ..Default::default()
    };
    config.save(&config_path).unwrap();
    let config_before = fs::read(&config_path).unwrap();
    let git_before = git_state(&fixture);
    let saved = json!({"kind":"observed navigation intent","private_original":"Keep42 exact source selection"});
    {
        let cache = Cache::open(&database(&fixture)).unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        cache
            .put_draft(&scope, "exact-avatar-draft", "Original42 unsent text")
            .unwrap();
        cache
            .save_view(&scope, "exact-avatar-intent", &saved)
            .unwrap();
    }
    // Optional public executable exists. Any attempted raw/bad or hidden actor
    // request would create its actual PID/argv journals and fail the zero oracle.
    let _assets = assets(&fixture);
    let trace_path = fixture.root.join("avatar-git-trace.jsonl");
    let (cols, rows) = if mouse { (80, 24) } else { (120, 40) };
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        cols,
        rows,
        &[("GIT_TRACE2_EVENT", trace_path.to_str().unwrap())],
    );
    ready(
        &mut session,
        &[
            "Recent commits",
            "author Different Author",
            "committer Current Committer",
        ],
    );
    choose(&mut session, mouse, "1 Inbox", b"1");
    ready(&mut session, &["Inbox • focused view"]);
    choose(&mut session, mouse, "[r Refresh]", b"r");
    ready(
        &mut session,
        &[
            RED_TITLE,
            GREEN_TITLE,
            "unknown actor [unknown]",
            "@same-login",
            // The unknown actor's list-row chip, then its label.
            "?   unknown actor [unknown]",
        ],
    );
    session.wait_for_command_count(&fixture, 7);
    assert_commands(&session, &fixture, 7);
    let cache = Cache::open(&database(&fixture)).unwrap();
    let scope = Scope::github("github.com", Some(42)).unwrap();
    let records = cache.list(&scope, None).unwrap();
    let bad = records
        .iter()
        .find(|record| record.kind == "pr" && record.payload["number"] == 1)
        .expect("Actual original PR record");
    assert_eq!(
        bad.payload["author"]["display_name"]
            .as_str()
            .unwrap()
            .len(),
        4097,
        "Raw observation is NOT truncated to create a different identity"
    );
    assert_eq!(
        bad.payload["author"]["avatar_url"], RED_URL,
        "Observed URL is retained, not guessed/replaced"
    );
    let before_input_trace = trace(&fixture);
    choose(&mut session, mouse, "[c Created by me]", b"c");
    ready(
        &mut session,
        &[
            "Queue: Created by me",
            RED_TITLE,
            GREEN_TITLE,
            "unknown actor [unknown]",
        ],
    );
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    ready(
        &mut session,
        &[RED_TITLE, GREEN_TITLE, "unknown actor [unknown]"],
    );
    choose(&mut session, mouse, "[a All]", b"a");
    ready(
        &mut session,
        &[
            "Queue: All",
            RED_TITLE,
            GREEN_TITLE,
            "unknown actor [unknown]",
        ],
    );
    session.focus_visible_action("[q Quit]");
    assert_commands(&session, &fixture, 7);
    assert_eq!(trace(&fixture), before_input_trace);
    assert_asset_journal(&fixture, false);
    assert!(!fixture.root.join("config/ghtui/avatars").exists());
    assert!(!session.screen().contains('▀'));
    assert_eq!(
        Cache::open(&database(&fixture))
            .unwrap()
            .list(&scope, None)
            .unwrap(),
        records,
        "Pure bounded actor display/input preserves source SQL frame"
    );
    assert_durable_intent(&fixture, &saved);
    choose(&mut session, mouse, "2 Repository", b"2");
    ready(
        &mut session,
        &[
            "Repository • focused view",
            "author Different Author",
            "committer Current Committer",
        ],
    );
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("Repository • focused view")
                    && screen.contains("author Different Author")
                    && screen.contains("committer Current Committer")
                    && !screen.contains("@same-login")
            },
            "Complete local frame has erased previous remote actor cells",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!session.screen().contains("@same-login"));
    assert_asset_journal(&fixture, false);
    assert_commands(&session, &fixture, 7);
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(git_state(&fixture), git_before);
    assert_read_only_git(&fixture);
    assert_durable_intent(&fixture, &saved);
    assert_eq!(fs::read(&config_path).unwrap(), config_before);
}
#[test]
fn actor_avatar_projection_keyboard_giant_observed_actor_keeps_raw_identity_and_responsive_fallback(
) {
    journey(false)
}
#[test]
fn actor_avatar_projection_mouse_giant_observed_actor_keeps_raw_identity_and_responsive_fallback() {
    journey(true)
}
