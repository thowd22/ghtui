//! Keyboard/mouse journeys through genuine same-name ID and node contradictions.
use super::*;

const CONTRADICTORY_BODY: &str = "PRIVATE_RECREATED_REPOSITORY_SEARCH_BODY";
fn contradiction_open(node: bool) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    let metadata = if node {
        json!({"id":10000,"node_id":"REPLACED_SAME_NAME_REPOSITORY_NODE","full_name":FIRST})
    } else {
        json!({"id":99999,"node_id":"REPO_catalogue_10000","full_name":FIRST})
    };
    steps.push(query(json!({"total_count":1,"incomplete_results":false,"items":[{
        "id":2007,"node_id":"ACTUAL_ISSUE_SEARCH_2007_NOT_REPO_ID","number":7,
        "title":"Must not admit replaced repository","body":CONTRADICTORY_BODY,
        "repository_url":format!("https://{HOST}/api/v3/repos/{FIRST}"),
        "html_url":format!("https://{HOST}/{FIRST}/pull/7"),
        "url":format!("https://{HOST}/api/v3/repos/{FIRST}/issues/7"),
        "pull_request":{"url":format!("https://{HOST}/api/v3/repos/{FIRST}/pulls/7"),"html_url":format!("https://{HOST}/{FIRST}/pull/7")},
        "state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-14T00:00:00Z",
        "user":{"id":51,"login":"unadmitted-metadata-actor","type":"User"},"repository":metadata
    }]})));
    assert_eq!(
        steps.len(),
        6,
        "pre2/repo1/inneridentity2/firstquery1; no second queue or closing/sourceSQL proof"
    );
    steps
}
fn metadata_journey(mouse: bool, node: bool) {
    let mut steps = collection(vec![json!([repository(0)])], 42);
    steps.extend(contradiction_open(node));
    assert_eq!(steps.len(), 13);
    let fixture = configured(&steps);
    let original = seed_original(&fixture);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    session.wait_for_command_count(&fixture, 7);
    ready(
        &mut session,
        &[TITLE, FIRST, "Pinned: yes", "Repository ID: 10000"],
    );
    preserved(&fixture, &original);
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        TITLE,
        &["[Esc Close]", "[q Quit]"],
    );
    let old = session
        .locate("[Open My PRs]")
        .expect("actual current action geometry");
    choose(&mut session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_command_count(&fixture, 13);
    hidden(&mut session);
    assert!(!session.screen().contains(CONTRADICTORY_BODY));
    assert!(!session.screen().contains("unadmitted-metadata-actor"));
    let gh = session.inspect_command_journal(&fixture);
    let git = git_commands(&fixture);
    preserved(&fixture, &original);
    if mouse {
        session.click(old.0, old.1);
    } else {
        session.send_key(b"arp");
    }
    session.resize_owned_modal(
        if mouse { 80 } else { 120 },
        if mouse { 24 } else { 40 },
        TITLE,
        &["[Esc Close]", "[q Quit]"],
    );
    hidden(&mut session);
    assert_eq!(session.inspect_command_journal(&fixture), gh);
    assert_eq!(git_commands(&fixture), git);
    preserved(&fixture, &original);
    // Esc is the same actual Close action without trusting stale Tab underline
    // geometry; the counterpart uses the actual visible clickable Close.
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    ready(
        &mut session,
        &["Repository • focused view", "Recent commits"],
    );
    for (name, label, key) in [
        ("Inbox", "1 Inbox", b"1".as_slice()),
        ("Actions", "4 Actions", b"4".as_slice()),
        ("Search", "3 Search", b"3".as_slice()),
        ("Dashboard", "5 Dashboard", b"5".as_slice()),
    ] {
        choose(&mut session, mouse, label, key);
        session.try_wait_for_condition(|screen|screen.contains(&format!("{name} • focused view"))&&normalized(screen).contains("Original repository scope is fenced • private pages hidden • no fallback or query sent"),"Actual static fenced view after inconsistent optional repository metadata",Duration::from_secs(8)).unwrap();
        choose(&mut session, mouse, "[r Refresh]", b"r");
        ready(&mut session, &["private pages hidden"]);
        assert!(
            !session.screen().contains(OLD_BODY) && !session.screen().contains(CONTRADICTORY_BODY)
        );
        assert_eq!(
            session.inspect_command_journal(&fixture),
            gh,
            "no default/all query after metadata failure"
        );
        assert_eq!(git_commands(&fixture), git);
        preserved(&fixture, &original);
    }
    choose(&mut session, mouse, "2 Repository", b"2");
    ready(&mut session, &["Recent commits"]);
    assert_eq!(git_state(&fixture), before);
    preserved(&fixture, &original);
    finish(&mut session, &fixture, mouse, 13, &before, &config);
    preserved(&fixture, &original);
    let actual = commands(&session, &fixture);
    for (row, step) in actual.iter().zip(&steps) {
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
    }
}
#[test]
fn repo_catalog_inbox_repository_metadata_keyboard_supplied_id_and_node_contradictions_hide_private(
) {
    for node in [false, true] {
        metadata_journey(false, node);
    }
}
#[test]
fn repo_catalog_inbox_repository_metadata_mouse_supplied_id_and_node_contradictions_hide_private() {
    for node in [false, true] {
        metadata_journey(true, node);
    }
}
