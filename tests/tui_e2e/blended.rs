//! Registered real-binary blended discovery journeys. Every GhStep is an exact
//! read, never an accepted-by-default fixture; live protocol counts are verified
//! independently of visible state and durable local effects.
#[path = "blended_privacy.rs"]
mod privacy_tests;
use crate::hydration_fixture as gh_fixture;
use crate::support::{Fixture, GhStep, Session};
use ghtui::{
    config::Config,
    storage::{Cache, Scope},
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs};

const NEEDLE: &str = "BlendAgreementNeedle";
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
fn close(session: &mut Session, mouse: bool, title: &str) {
    // A control label can already be visible behind a newly requested overlay.
    // Prove that exact overlay was rendered before sending Close; otherwise a
    // negative-only disappearance check can pass before Open was processed.
    session.wait_for_screen(title);
    if mouse {
        session.close_overlay_by_mouse(title);
    } else {
        session.close_overlay_by_key(title);
    }
}
fn choose_control(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    let title = "Blend controls • explicit Ranked GitHub discovery";
    session.wait_for_screen(title);
    session.wait_for_screen(label);
    if mouse {
        let (_, top) = session.locate(title).unwrap();
        let screen = session.screen();
        let (row, line) = screen
            .lines()
            .enumerate()
            .find(|(row, line)| *row > usize::from(top) && line.contains(label))
            .unwrap_or_else(|| panic!("Active blend control absent: {label}\n{screen}"));
        let byte = line.find(label).unwrap();
        let column = ghtui::editor::display_width(&line[..byte]);
        session.click(column as u16, row as u16);
    } else {
        session.send_key(key);
    }
}
fn query(session: &mut Session, mouse: bool, input: &str) {
    choose(session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste(input);
    choose(session, mouse, "[Enter Search]", b"\r");
}
fn fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "tracked.txt",
        b"unrelated original saved bytes\n",
        "Original local history",
    );
    fs::write(
        fixture.repo().join("tracked.txt"),
        format!("{NEEDLE} local dirty working bytes\n"),
    )
    .unwrap();
    Config {
        host: gh_fixture::HOST.into(),
        avatars: false,
        active_set: Some("selected".into()),
        repository_sets: BTreeMap::from([("selected".into(), vec![gh_fixture::REPOSITORY.into()])]),
        local_repositories: BTreeMap::from([(gh_fixture::REPOSITORY.into(), fixture.repo())]),
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    fixture
}
fn scope() -> Scope {
    Scope::github(gh_fixture::HOST, Some(42)).unwrap()
}
fn cache(fixture: &Fixture) -> Cache {
    Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap()
}
fn search_step(term: &str, items: Vec<Value>, total: u64, per_page: usize) -> GhStep {
    GhStep::read(
        &[
            "api",
            "search/issues",
            "--hostname",
            gh_fixture::HOST,
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.text-match+json",
            "-f",
            &format!("q={term} repo:{} is:pr", gh_fixture::REPOSITORY),
            "-f",
            &format!("per_page={per_page}"),
            "-f",
            "page=1",
        ],
        &json!({"total_count":total,"incomplete_results":false,"items":items}).to_string(),
    )
}
fn indexed_parent(node: &str, number: u64) -> Value {
    let mut parent = gh_fixture::pr(&format!(
        "{NEEDLE} stale indexed bytes which must not replace current body"
    ));
    parent["node_id"] = json!(node);
    parent["number"] = json!(number);
    parent["id"] = json!(if node == gh_fixture::PR_NODE {
        700
    } else {
        700 + number
    });
    parent["title"] = json!(format!("{NEEDLE} remote parent {number}"));
    parent["url"] = json!(format!(
        "https://{}/api/v3/repos/{}/issues/{number}",
        gh_fixture::HOST,
        gh_fixture::REPOSITORY
    ));
    parent["html_url"] = json!(format!(
        "https://{}/{}/pull/{number}",
        gh_fixture::HOST,
        gh_fixture::REPOSITORY
    ));
    parent["repository"] = gh_fixture::repository();
    parent["pull_request"] = json!({"url":format!("https://{}/api/v3/repos/{}/pulls/{number}",gh_fixture::HOST,gh_fixture::REPOSITORY)});
    parent["text_matches"] = json!([{"fragment":format!("{NEEDLE} stale indexed fragment"),"property":"body","object_type":"IssueComment","matches":[{"indices":[0,20]}]}]);
    parent
}
fn start(session: &mut Session, mouse: bool) {
    session.wait_for_screen("Recent commits");
    choose(session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
}
fn load_parent(session: &mut Session, fixture: &Fixture, mouse: bool, expected_count: usize) {
    choose(session, mouse, "[H Search sources]", b"H");
    session.wait_for_screen("Search sources • explicit read-only hydration");
    choose(session, mouse, "[p PR]", b"p");
    choose(session, mouse, "[v PR/issue number]", b"v");
    session.send_key(b"\x01");
    session.paste("7");
    session.send_key(b"\r");
    choose(session, mouse, "[l Load source]", b"l");
    journal(session, fixture, expected_count);
    session.wait_for_screen("Loaded 1 cached discussion records");
    close(
        session,
        mouse,
        "Search sources • explicit read-only hydration",
    );
}
fn rank_off(session: &mut Session, fixture: &Fixture, mouse: bool, count: &mut usize) {
    choose(session, mouse, "[n Ranked]", b"n");
    session.wait_for_screen("Ranked mode");
    journal(session, fixture, *count);
    query(session, mouse, NEEDLE);
    *count += 6;
    journal(session, fixture, *count);
    session.wait_for_screen("2 results • Ranked");
}
fn preference(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    count: &mut usize,
    label: &str,
    key: &[u8],
    next: &str,
) {
    choose_control(session, mouse, label, key);
    *count += 4;
    journal(session, fixture, *count);
    let saved = cache(fixture)
        .saved_view(&scope(), "search-blend-policy:v1")
        .unwrap()
        .unwrap();
    let saved: ghtui::search::fusion::blend::BlendPolicy =
        serde_json::from_value(saved["policy"].clone()).unwrap();
    session.wait_for_screen(&ghtui::search_blend::saved_status(&saved));
    session.wait_for_screen(next);
}
fn enable_pr_only(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    count: &mut usize,
    caps: bool,
) {
    choose(session, mouse, "[B Blend controls]", b"B");
    session.wait_for_screen("[b Blend GitHub: off]");
    let before = *count;
    preference(
        session,
        fixture,
        mouse,
        count,
        "[c Code: on]",
        b"c",
        "[c Code: off]",
    );
    preference(
        session,
        fixture,
        mouse,
        count,
        "[i Issues: on]",
        b"i",
        "[i Issues: off]",
    );
    if caps {
        preference(
            session,
            fixture,
            mouse,
            count,
            "[f Final results: 100 less]",
            b"f",
            "[f Final results: 99 less]",
        );
        preference(
            session,
            fixture,
            mouse,
            count,
            "[- Results/source: 100 less]",
            b"-",
            "[- Results/source: 99 less]",
        );
        preference(
            session,
            fixture,
            mouse,
            count,
            "[[ Pages/source: 2 less]",
            b"[",
            "[[ Pages/source: 1 less]",
        );
    }
    preference(
        session,
        fixture,
        mouse,
        count,
        "[b Blend GitHub: off]",
        b"b",
        "[b Blend GitHub: on]",
    );
    if caps {
        // Disabled minimum and final selected-provider controls use the same
        // real input routes without saving or issuing any search request.
        let saved = cache(fixture)
            .saved_view(&scope(), "search-blend-policy:v1")
            .unwrap()
            .unwrap();
        choose_control(session, mouse, "[[ Pages/source: 1 less]", b"[");
        session.wait_for_screen("Action unavailable • explanation only");
        close(session, mouse, "Action unavailable • explanation only");
        choose(session, mouse, "[B Blend controls]", b"B");
        session.wait_for_screen("[b Blend GitHub: on]");
        choose_control(session, mouse, "[p PRs: on]", b"p");
        session.wait_for_screen("Action unavailable • explanation only");
        close(session, mouse, "Action unavailable • explanation only");
        choose(session, mouse, "[B Blend controls]", b"B");
        session.wait_for_screen("[b Blend GitHub: on]");
        journal(session, fixture, *count);
        assert_eq!(
            cache(fixture)
                .saved_view(&scope(), "search-blend-policy:v1")
                .unwrap()
                .unwrap(),
            saved
        );
    }
    let rows = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(
        rows[before..]
            .iter()
            .all(|row| row["args"][0] == "--version" || row["args"][1] == "user"),
        "preferences never issue search requests"
    );
    close(
        session,
        mouse,
        "Blend controls • explicit Ranked GitHub discovery",
    );
    session.wait_for_screen("[B Blend controls]");
}
fn policy_steps(caps: bool) -> Vec<GhStep> {
    (0..if caps { 6 } else { 3 })
        .flat_map(|_| gh_fixture::private_read(42))
        .collect()
}
fn blend_steps(search: GhStep) -> Vec<GhStep> {
    blend_steps_selected(search, true)
}
fn blend_steps_selected(search: GhStep, selected: bool) -> Vec<GhStep> {
    let mut steps = gh_fixture::private_read(42); // main before + worker before.
    if selected {
        steps.push(gh_fixture::read(
            "repos/example/repo",
            gh_fixture::repository(),
        ));
    }
    let parents = search.status == 0
        && serde_json::from_slice::<Value>(&search.stdout).unwrap()["items"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty());
    steps.push(search);
    if parents {
        steps.push(gh_fixture::read(
            "repos/example/repo",
            gh_fixture::repository(),
        ));
    }
    if parents || selected {
        steps.push(gh_fixture::read(
            "repos/example/repo",
            gh_fixture::repository(),
        ));
    }
    steps.extend(gh_fixture::private_read(42)); // worker after + main after.
    assert_eq!(
        steps.len(),
        9 + usize::from(selected) + usize::from(parents) + usize::from(parents || selected)
    );
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
    assert!(rows.iter().all(|row| row["matched"] == true));
    assert!(rows.iter().all(|row| row["args"][0] == "--version"
        || row["args"][5] == "GET"
        || row["args"][1] == "graphql" && row["args"][5] == "POST"));
}
fn current_parent(fixture: &Fixture, expected: &str) {
    let records = cache(fixture)
        .list(&scope(), Some("collaboration-document-v1"))
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].payload["body"], expected);
    let saved = cache(fixture)
        .saved_view(&scope(), "search-source-selection:v1")
        .unwrap()
        .unwrap();
    let sources: ghtui::search::sources::MetadataSources =
        serde_json::from_value(saved["sources"].clone()).unwrap();
    assert_eq!(sources.discussions.len(), 4);
    for target in sources.discussions {
        let cache = cache(fixture);
        let stored = cache
            .sync_checkpoint(&scope(), &target.source())
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(
            cache
                .binding_status(&scope(), &target.source())
                .unwrap()
                .unwrap()
                .visible_generation,
            Some(stored)
        );
    }
}

fn rank_sources(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize) {
    let commands = session.inspect_command_journal(fixture);
    let policy = cache(fixture)
        .saved_view(&scope(), "search-blend-policy:v1")
        .unwrap()
        .unwrap();
    let records = cache(fixture).list(&scope(), None).unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    choose(session, mouse, "[B Blend controls]", b"B");
    session.wait_for_screen("[f Final results: 99 less]");
    choose(session, mouse, "[E Rank sources]", b"E");
    session.wait_for_screen("Rank sources • limits and coverage");
    session.wait_for_screen("RRF k=60 uses source positions");
    session.wait_for_screen("99 loaded / 99 admitted / 0 hidden");
    session.resize(80, 12);
    choose(session, mouse, "[PgDn More]", b"\x1b[6~");
    if let Err(error) = session.try_wait_for_condition(
        |screen| !screen.contains("Independent rank lists"),
        "prepared rank provenance scroll moved",
        std::time::Duration::from_secs(8),
    ) {
        session.save_artifacts();
        panic!("{error}");
    }
    choose(session, mouse, "[PgUp Previous]", b"\x1b[5~");
    session.wait_for_screen("Independent rank lists");
    if mouse {
        session.wheel(30, 7, true);
        if let Err(error) = session.try_wait_for_condition(
            |screen| !screen.contains("Independent rank lists"),
            "prepared rank provenance mouse wheel moved",
            std::time::Duration::from_secs(8),
        ) {
            session.save_artifacts();
            panic!("{error}");
        }
        session.wheel(30, 7, false);
        session.wait_for_screen("Independent rank lists");
    }
    close(session, mouse, "Rank sources • limits and coverage");
    session.resize(80, 24);
    journal(session, fixture, count);
    assert_eq!(session.inspect_command_journal(fixture), commands);
    assert_eq!(
        cache(fixture)
            .saved_view(&scope(), "search-blend-policy:v1")
            .unwrap()
            .unwrap(),
        policy
    );
    assert_eq!(cache(fixture).list(&scope(), None).unwrap(), records);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
}

#[test]
fn collaboration_index_blended_ranked_keyboard_explicit_scopes_caps_canonical_rrf_and_exact_context(
) {
    happy(false);
}
#[test]
fn collaboration_index_blended_ranked_mouse_explicit_scopes_caps_canonical_rrf_and_exact_context() {
    happy(true);
}
fn happy(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let body=format!("{NEEDLE} current canonical body, not the indexed comment fragment\nSecond original body line");
    let mut steps = gh_fixture::pr_load(&gh_fixture::pr(&body), vec![], vec![], vec![], 42);
    steps.extend(gh_fixture::private_source_read(42)); // Explicit cached Ranked query, remote off.
    steps.extend(policy_steps(true));
    let mut matches = vec![indexed_parent(gh_fixture::PR_NODE, 7)];
    matches.extend((20..118).map(|number| indexed_parent(&format!("PR_remote_{number}"), number)));
    assert_eq!(matches.len(), 99);
    steps.extend(blend_steps(search_step(NEEDLE, matches, 300, 99)));
    steps.extend(gh_fixture::private_source_read(42)); // Exact cached canonical source Open.
    assert_eq!(steps.len(), 77);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    query(&mut session, mouse, NEEDLE);
    session.wait_for_screen("1 results • Exact");
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "No opt-in/principal means local-only exact content, no remote discovery"
    );
    load_parent(&mut session, &fixture, mouse, 29);
    current_parent(&fixture, &body);
    let mut count = 29;
    rank_off(&mut session, &fixture, mouse, &mut count);
    enable_pr_only(&mut session, &fixture, mouse, &mut count, true);
    query(&mut session, mouse, NEEDLE);
    session.wait_for_screen("99 results • Ranked");
    session.wait_for_screen("RRF k=60");
    session.wait_for_screen("GitHub PRs #1");
    session.wait_for_screen("PARTIAL");
    session.wait_for_screen("current canonical body");
    count += 12;
    journal(&mut session, &fixture, count);
    rank_sources(&mut session, &fixture, mouse, count);
    session.resize(80, 24);
    choose(&mut session, mouse, "[o Open result]", b"o");
    session.wait_for_screen("Exact search source • read-only");
    session.wait_for_screen(gh_fixture::PR_NODE);
    session.wait_for_screen("current canonical body");
    assert!(!session.screen().contains("stale indexed fragment"));
    count += 6;
    journal(&mut session, &fixture, count);
    assert_eq!(count, steps.len());
    current_parent(&fixture, &body);
    let policy = cache(&fixture)
        .saved_view(&scope(), "search-blend-policy:v1")
        .unwrap()
        .unwrap();
    assert_eq!(policy["version"], 1);
    assert_eq!(policy["policy"]["enabled"], true);
    assert_eq!(policy["policy"]["per_source_pages"], 1);
    assert_eq!(policy["policy"]["per_source_results"], 99);
    assert_eq!(policy["policy"]["max_results"], 99);
    assert_eq!(policy["policy"]["sources"], json!(["github_pull_requests"]));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        format!("{NEEDLE} local dirty working bytes\n").as_bytes()
    );
    close(&mut session, mouse, "Exact search source • read-only");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn collaboration_index_blended_ranked_keyboard_rate_limit_retains_current_local_without_remote_empty_claim(
) {
    partial(false);
}
#[test]
fn collaboration_index_blended_ranked_mouse_rate_limit_retains_current_local_without_remote_empty_claim(
) {
    partial(true);
}
fn partial(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let body = format!("{NEEDLE} current locally authorized parent");
    let mut steps = gh_fixture::pr_load(&gh_fixture::pr(&body), vec![], vec![], vec![], 42);
    steps.extend(gh_fixture::private_source_read(42));
    steps.extend(policy_steps(false));
    let mut unavailable = search_step(NEEDLE, vec![], 0, 100);
    unavailable.status = 1;
    unavailable.stdout.clear();
    unavailable.stderr = b"gh: API rate limit exceeded (HTTP 429)\n".to_vec();
    steps.extend(blend_steps(unavailable));
    assert_eq!(steps.len(), 58);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    query(&mut session, mouse, NEEDLE);
    session.wait_for_screen("1 results • Exact");
    load_parent(&mut session, &fixture, mouse, 29);
    let mut count = 29;
    rank_off(&mut session, &fixture, mouse, &mut count);
    enable_pr_only(&mut session, &fixture, mouse, &mut count, false);
    query(&mut session, mouse, NEEDLE);
    session.wait_for_screen("2 results • Ranked");
    session.wait_for_screen("PARTIAL");
    session.wait_for_screen("rate limit");
    session.wait_for_screen("current locally authorized");
    count += 11;
    journal(&mut session, &fixture, count);
    assert_eq!(count, steps.len());
    current_parent(&fixture, &body);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn collaboration_index_blended_ranked_keyboard_removed_parent_alias_outside_intent_and_fresh_recovery(
) {
    removed(false);
}
#[test]
fn collaboration_index_blended_ranked_mouse_removed_parent_alias_outside_intent_and_fresh_recovery()
{
    removed(true);
}
fn removed(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let original = format!("{NEEDLE} RemoveFencePrivateCanary");
    let recovered = format!("{NEEDLE} BlendRecoveryVisibleCanary");
    let mut steps = gh_fixture::pr_load(&gh_fixture::pr(&original), vec![], vec![], vec![], 42);
    steps.extend(gh_fixture::private_source_read(42));
    steps.extend(policy_steps(false));
    steps.extend(gh_fixture::account(42)); // Existing exact Source Remove local visibility effect.
    steps.extend(blend_steps_selected(
        search_step(NEEDLE, vec![indexed_parent(gh_fixture::PR_NODE, 7)], 1, 100),
        false,
    ));
    steps.extend(gh_fixture::pr_load(
        &gh_fixture::pr(&recovered),
        vec![],
        vec![],
        vec![],
        42,
    ));
    steps.extend(blend_steps(search_step(
        NEEDLE,
        vec![indexed_parent(gh_fixture::PR_NODE, 7)],
        1,
        100,
    )));
    assert_eq!(steps.len(), 101);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    load_parent(&mut session, &fixture, mouse, 29);
    current_parent(&fixture, &original);
    let mut count = 29;
    rank_off(&mut session, &fixture, mouse, &mut count);
    enable_pr_only(&mut session, &fixture, mouse, &mut count, false);
    choose(&mut session, mouse, "[H Search sources]", b"H");
    session.wait_for_screen("Search sources • explicit read-only hydration");
    choose(&mut session, mouse, "[p PR]", b"p");
    choose(&mut session, mouse, "[v PR/issue number]", b"v");
    session.send_key(b"\x01");
    session.paste("7");
    session.send_key(b"\r");
    let selected = cache(&fixture)
        .saved_view(&scope(), "search-source-selection:v1")
        .unwrap()
        .unwrap();
    let selected: ghtui::search::sources::MetadataSources =
        serde_json::from_value(selected["sources"].clone()).unwrap();
    assert_eq!(selected.discussions.len(), 4);
    choose(&mut session, mouse, "[d Remove source]", b"d");
    session.wait_for_screen("Removed 4 selected sources");
    count += 2;
    journal(&mut session, &fixture, count);
    let empty = cache(&fixture)
        .saved_view(&scope(), "search-source-selection:v1")
        .unwrap()
        .unwrap();
    let empty: ghtui::search::sources::MetadataSources =
        serde_json::from_value(empty["sources"].clone()).unwrap();
    assert!(
        empty.discussions.is_empty(),
        "removed parents are outside selected intent"
    );
    for target in &selected.discussions {
        assert_eq!(
            cache(&fixture)
                .binding_status(&scope(), &target.source())
                .unwrap()
                .unwrap()
                .visible_generation,
            None
        );
    }
    assert!(
        cache(&fixture)
            .list(&scope(), Some("collaboration-document-v1"))
            .unwrap()
            .iter()
            .any(|record| record.payload["body"] == original),
        "visibility Remove preserves recoverable canonical bytes"
    );
    close(
        &mut session,
        mouse,
        "Search sources • explicit read-only hydration",
    );
    query(&mut session, mouse, NEEDLE);
    count += 11;
    journal(&mut session, &fixture, count);
    session.wait_for_screen("1 results • Ranked");
    session.wait_for_screen("PARTIAL");
    session.wait_for_screen("local dirty working bytes");
    assert!(!session.screen().contains("RemoveFencePrivateCanary"));
    assert!(!session.screen().contains("stale indexed fragment"));
    session.resize(80, 24);
    count += 29;
    load_parent(&mut session, &fixture, mouse, count);
    journal(&mut session, &fixture, count);
    current_parent(&fixture, &recovered);
    query(&mut session, mouse, NEEDLE);
    count += 12;
    journal(&mut session, &fixture, count);
    session.wait_for_screen("2 results • Ranked");
    session.wait_for_screen("BlendRecoveryVisibleCanary");
    assert!(!session.screen().contains("RemoveFencePrivateCanary"));
    assert!(!session.screen().contains("stale indexed fragment"));
    assert_eq!(count, steps.len());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        format!("{NEEDLE} local dirty working bytes\n").as_bytes()
    );
    session.quit_and_assert_restored_terminal(mouse);
}
